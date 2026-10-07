# plix — gradually typed brace language with a tree-walking interpreter and a Cranelift AOT compiler over one shared Rust runtime

`research/prior-art/repos/plix` · MIT (`LICENSE:1`) · last commit 2026-09-18 · Rust: `src/` 25.9k lines
(core pipeline 15.2k: `lexer.rs` 574, `parser.rs` 1594, `ast.rs` 338, `typecheck.rs` 2555,
`owncheck.rs` 968, `resolve.rs` 920, `macros.rs` 408, `interp.rs` 1549, `codegen.rs` 4216,
`jit.rs` 503, `wasm.rs` 1355; the rest is CLI, LSP, package manager, registry, docker wrapper),
runtime `rt/src/` 10.2k (`builtins.rs` 4229, `value.rs` 1243, `heap.rs` 1224, `pyffi.rs` 890),
`build.rs` 180; 25.2k lines of `.px` tests and examples. Maturity: v0.10.1, pre-1.0, one
developer's scale with a very wide surface; several advertised parts are facades (a JIT that never
runs code, a test directory that does not exist), and its own benchmarks put the native backend
behind CPython on calls (`docs/comparison.md:46-57`).

All paths below are relative to `research/prior-art/repos/plix/` unless they start with `compiler/` or
`docs/adr/` (LotML).

## Architecture

There is **no IR**. Both execution paths walk the same AST; the checker communicates with them
through side tables and bit flags written on AST nodes.

| Stage | Where | Key types |
|---|---|---|
| Lex | `src/lexer.rs:35` `lex()` | `token::Tok`, `Span`, `StrPart` (interpolation) |
| Parse | `src/parser.rs:37` `parse_file()` | `ast::Node<T>{node, span, flags: Cell<u8>}` `src/ast.rs:7-14`; `Stmt`/`Expr` `ast.rs:43-44` |
| Macros | `src/macros.rs` (parser-level expansion + re-parse) | `MacroDef` |
| Types | `src/typecheck.rs:358` `check_program()` | `Ty` `typecheck.rs:37-56`, `TypeInfo` `typecheck.rs:153-163` |
| Ownership | `src/owncheck.rs` `check_program()` | `OwnError` `owncheck.rs:24-30`, states `St` `owncheck.rs:33-38` |
| Names (native only) | `src/resolve.rs:512` `resolve_program_with_base()` | `FnRes{captures, cell_vars, locals}` `resolve.rs:26-30` |
| Interpret | `src/interp.rs:1263` `run_program()` | `Interpreter` `interp.rs:200-206`, `Scope` env chain `interp.rs:33-38` |
| Native | `src/codegen.rs:258` `build_object()` → `link_executable()` `codegen.rs:4002/4042` | `Emit`, `FEnv`, `Loc`, `STy` `codegen.rs:586-676` |
| WASM | `src/wasm.rs:20` `compile_source()` (parse → emit, **no checker**) | `WasmGen` |
| Runtime | `rt/` crate `plixrt`, `crate-type = ["staticlib","rlib"]` `rt/Cargo.toml` | `V = u64` `rt/src/heap.rs:10`, `HeapObj` `heap.rs:63-96` |

How `run` and `build` share:

- **Shared**: lexer, parser, AST, macro expansion, `typecheck` and `owncheck` (both entry points
  call them in the same order: `interp.rs:1269-1278`, `codegen.rs:259-268`), and the **runtime**.
  `plixrt` is linked into the `plix` binary as an rlib for the interpreter and embedded as a
  staticlib into every native executable (`rt/src/lib.rs:13-17`). Every non-trivial operation in
  native code is a call to the same Rust function the interpreter calls (`value::add` etc. wrapped
  as `extern "C" plix_add` by the `wrap!` macro, `rt/src/value.rs:957-983`). That is the real
  sharing mechanism: one runtime, two drivers.
- **Not shared**: control flow, scoping, calls, closures, typed-slot guards, overflow rules. Each
  is implemented twice, once in `interp.rs` and once in `codegen.rs`, and kept "in lockstep" by
  comments and a differential test battery. `resolve.rs` runs only on the native path
  (`codegen.rs:323-331`; the interpreter never calls it).
- The checker hands results to both drivers by mutating the AST: `FLAG_STRICT_INT_ARITH`,
  `FLAG_GUARD_INT/FLOAT/BOOL/NULLABLE` (`ast.rs:26-41`, set in `typecheck.rs:1593,1636`), plus
  `TypeInfo.local_tys` (provably scalar locals, `typecheck.rs:161-162`).
- `plix exec` = `build` to a temp dir then spawn (`codegen.rs:230-240`), not a JIT.

## Frontend

- Hand-written lexer and recursive-descent parser with precedence climbing (`parser.rs:5`); C-like
  syntax with braces and semicolons, no significant indentation. `no_struct_lit` flag resolves
  `match p { ... }` vs struct literal (`parser.rs:30-32`).
- No error recovery: `parse_file` returns the first `ParseError` (`parser.rs:20-24,37`). The
  checker, by contrast, collects every error in one pass and dedupes identical diagnostics
  (`typecheck.rs:192-206`).
- No incrementality, no salsa: every `build` re-parses and re-checks every imported module
  (`codegen.rs:275-316`). Only a whole-program object cache exists (see Pitfalls).
- AST nodes carry interior-mutable checker flags (`ast.rs:10-13`); `FuncDef` identity is the `Rc`
  address (`FuncDef::id`, used as map key throughout `codegen.rs:386-399`).

Nothing here improves on `lotml-syntax` (tolerant parser, indentation, salsa via `lotml-db`).

## Semantics and types

- **Gradual typing** (`typecheck.rs:5-24`, `docs/typing.md`): unannotated = `Ty::Any`; `Any` is
  compatible both ways (`typecheck.rs:945`); `int` widens to `float`; a provable mismatch at an
  annotated boundary is a hard error with Rust-style codes (E0308, E0061, ...). No generics:
  `array<T>`/`map<K,V>` are checked at a few boundaries but erased (`docs/typing.md:138-142`).
- **Inference for unboxing**: each function body is walked twice, the second pass silent, "so that
  loop-carried demotions propagate" (`typecheck.rs:1235-1248`); surviving `Int|Float|Bool` locals
  go to `local_tys` (`typecheck.rs:1250-1265`). Two passes, not iterated to a fixpoint.
- **Boundary guards** (the gradual boundary, relevant to adr:0012): a dynamic value entering a
  typed slot is checked at run time at typed parameter binding, annotated declarations, typed
  for-in, assignment to typed locals and declared returns (`docs/typing.md:47-58`). Native emits
  `unbox_int_guard`/`unbox_float_guard` (`codegen.rs:763-808`), the interpreter `guard_typed`
  (`interp.rs:135-154`); the message comes from the runtime so both print the same text
  (`codegen.rs:784-785` → `rt heap::guard_msg_int`). **Only scalars are guarded**: the flag
  function returns 0 for every other annotation (`codegen.rs:633-643`, `interp.rs:167-177`), so
  `auto s: str = untyped()` or a struct-typed parameter accepts anything at run time. Element
  types are never checked. adr:0012 already goes further (integer ranges, element types, records,
  variants, plus a copy) — plix is evidence that scalar-only guards leave the boundary unsound.
- **Ownership** (`owncheck.rs:1-19`): `own` bindings get Rust-like move/borrow checks (E0382,
  E0499, E0503, ...), statically only. Codegen and interpreter never look at `VarKind::Own` (no
  match in `codegen.rs`/`interp.rs`): `own` values are reference-counted exactly like `auto`. The
  "zero cost" in `docs/memory.md` means "no extra cost", not "no RC".
- **Errors**: no exceptions in the language; `Result`/`Option` values and `match`
  (`tests/language/lang_exceptions_test.px:3-8`), but runtime faults abort through a thread-local
  error flag. `Result`/`Option`/nullable suites are interpreter-only (`tests/differential.sh:23-25`).
- Dynamic features kept, not refused: untyped code, first-class functions checked at call time,
  `type_of`, maps of `String → V`.

## IR and passes

None. The closest things to passes are AST walks inside `codegen.rs`:

- `expr_may_alloc`/`stmt_may_alloc` — decide whether a statement needs an arena checkpoint (`codegen.rs:73-160`).
- `collect_all_fn_defs` — declare every function up front so recursion resolves (`codegen.rs:370-399, 3683`).
- `collect_assigned_idents` — globals never reassigned allow direct calls (`codegen.rs:337-338, 3837`).
- `static_ty` — per-expression "can this be emitted unboxed" analysis, the single source of truth for `emit_raw` (`codegen.rs:833-971`).
- `find_stable_fn` — top-level, capture-free, never-reassigned functions get a direct `call` instead of `plix_call` (`codegen.rs:2922-2935, 2822-2853`).
- Cranelift's own optimizer at `opt_level=speed` (`codegen.rs:340-343`). No inlining (acknowledged, `docs/comparison.md:57-59`).

Compare: LotML's `lotml-ir` has mono, ownership, reuse, hoist and a verifier
(`compiler/crates/lotml-ir/src/`), shared by every backend (adr:0020).

## Backend and toolchain

- **Codegen**: Cranelift 0.122 (`cranelift-codegen` with `x86`+`arm64`, `-frontend`, `-module`,
  `-object`, `-native`; `Cargo.toml` deps). Host ISA only via `cranelift_native::builder()`
  (`codegen.rs:344-347`); `--target` accepts only `wasm` (`src/main.rs:756-781`). No
  cross-compilation, no shared libraries (LotML has `--shared`, adr:0024).
- **ABI of compiled functions**: every Plix function is `fn(cells: *const V, args: *const V, nargs: i64) -> V`
  (`codegen.rs:9-11, 366`); arguments are stored to a stack slot and passed by address
  (`stack_args`, `codegen.rs:739-750`); arity and defaults are checked in the callee
  (`codegen.rs:3385-3499`). Typed calls still box arguments and the return, then re-guard
  (`codegen.rs:1168-1200`).
- **Calling convention**: hard-coded `CallConv::SystemV` for every signature, including imported
  runtime functions and the exported C `main` (`codegen.rs:493-505, 3621-3627`; `jit.rs:187`).
  The runtime functions are Rust `extern "C"` (`rt/src/heap.rs:692-816`), and the runtime calls
  compiled closures back through `extern "C" fn` pointers (`rt/src/value.rs:909`). On
  x86_64-windows `extern "C"` is the Microsoft x64 convention, so both directions disagree there.
  See Pitfalls.
- **Object emission**: `ObjectModule` with `is_pic=true`, functions `Linkage::Local`
  (`plix_fn_<name>_<id>`), runtime symbols `Linkage::Import` declared lazily with an
  `nparams × i64 → i64` signature (`codegen.rs:519-530`) — not from the frozen ABI table
  (`src/abi_freeze.rs:33`), so declared and real signatures can drift (e.g. `plix_rt_init`
  returns void). LotML reads runtime signatures from `lotml.h` itself
  (`compiler/crates/lotml-runtime/src/abi.rs:1-3`), which is the better design.
- **Runtime embedding**: `build.rs` copies Cargo's `libplixrt.a` (`plixrt.lib` on MSVC) into
  `OUT_DIR`, or builds it with a nested `cargo build --target-dir <scratch>` to avoid deadlocking
  on the parent's lock (`build.rs:115-158`); `codegen.rs:3986-3989` `include_bytes!`s it, so the
  `plix` binary carries its runtime and `plix build` needs no Rust toolchain. At link time the
  ~23 MB archive is written to a temp dir and deleted afterwards (`codegen.rs:4003-4027`).
- **Linking, Unix**: `$CC` or `cc -o out plix_main.o libplixrt.a -lm -lpthread -ldl`
  (`codegen.rs:4009-4024`).
- **Linking, Windows** (`codegen.rs:4040-4108`): first `link.exe /NOLOGO obj lib /OUT:x.exe` found
  on `PATH` (so only from a Visual Studio developer prompt; no vswhere lookup), with no system
  import libraries named; if `link.exe` cannot be spawned, `$CC`/`cc` as MinGW with
  `-lws2_32 -ladvapi32 -lbcrypt -luserenv -lkernel32`. If a `link.exe` spawns but fails — e.g. Git
  for Windows' coreutils `link.exe` earlier on `PATH` — it reports that failure without trying
  MinGW (`codegen.rs:4063-4072`). The MinGW fallback links whatever archive flavor the `plix`
  binary was built with (`plixrt_embed.lib` for an MSVC-built `plix`, `codegen.rs:3988-3998`).
  `.exe` is appended automatically (`codegen.rs:4049-4053`).
- **Object cache**: generated objects cached by `DefaultHasher(src)` under a per-compiler
  fingerprint directory (`codegen.rs:174-207`); the fingerprint hashes compiler and runtime
  sources, version, target, rustc version, `build.rs`, `Cargo.lock` (`build.rs:57-87`).
- **"JIT"** (`src/jit.rs`): compiles an int-only subset to a byte buffer with
  `Context::compile` (`jit.rs:179-249`) but never maps it executable or calls it;
  `jit_eval_expr` returns the *length of the machine code* as its "result" (`jit.rs:465-469`);
  no caller outside the file; `#![allow(dead_code)]` (`jit.rs:2`). DeepWiki describes it as a
  working tiered JIT with hotspot promotion (`research/prior-art/deepwiki/plix.md:1635-1660`) — wrong.
- **WASM**: separate emitter from the unchecked AST, unsupported constructs "fall through to a safe
  default (0)" (`docs/wasm.md` §2, `wasm.rs:20-27`).

## Runtime

- **Values** (`rt/src/heap.rs:10-46`): `V = u64`; low bit 1 = 63-bit int (`(i<<1)|1`), `0` null,
  `2` true, `6` false, anything else a pointer to `HeapBox{rc: Cell<u32>, obj: UnsafeCell<HeapObj>}`
  (`heap.rs:98-101`). Ints outside 62 bits silently become floats (`mk_int`, `heap.rs:234-240`).
  **Floats are heap-allocated** (`mk_float`, `heap.rs:242-248`) except in unboxed typed locals.
- **Memory**: non-atomic reference counting + a per-frame "arena" of temporaries
  (`heap.rs:103-113, 507-558`). Every allocation is pushed onto the current frame's arena; every
  variable read through `plix_var_use` retains and pushes again (`heap.rs:587-601`); arenas rewind
  after each expression statement and loop iteration, and `plix_frame_pop` adopts the return value
  into the caller's arena (`heap.rs:512-533`). State lives in a thread-local `RefCell`
  (`heap.rs:108-128`), so every retain-to-arena is a TLS access plus a `RefCell` borrow plus a
  `Vec` push. No cycle collection, no weak refs (`docs/memory_limitations.md` §1); freeing is
  recursive (`heap.rs:172-230`).
- **Strings, lists, dicts**: `String`, `Vec<V>`, `HashMap<String, V>` inside `HeapObj`
  (`heap.rs:63-96`); maps are string-keyed only. Concat reuses the buffer when `rc == 1`
  (`heap.rs:258-262`) — a small instance of reuse-when-unique (adr:0008 does this systematically).
- **Errors**: a thread-local `LAST_ERROR` (`heap.rs:629-644`); every wrapped op returns 0 and sets
  it (`value.rs:957-977`); native code calls `plix_err_flag()` after each fallible op and branches
  to the function's error block (`codegen.rs:693-701`), which pushes a trace frame and returns null
  (`codegen.rs:3236-3249`).
- **Builtins**: one table shared by both drivers (`rt/src/builtins.rs`, `build_global_entries`
  used at `interp.rs:217-220`; `global_names` reserves the first global slots, `codegen.rs:318-322`).
- **CPython interop** (`rt/src/pyffi.rs`): `dlopen`/`LoadLibraryA` a list of candidate libpython
  names (`pyffi.rs:126-171`, override `PLIX_PYTHON_LIB`), resolve ~45 C-API symbols into a struct
  (`pyffi.rs:192-256`), GIL via `PyGILState_Ensure` (`pyffi.rs:282-291`). Scalars, strings,
  lists, dicts convert; anything else stays an opaque handle; numpy-style scalars convert via
  `.item()` (`pyffi.rs:519-565`). No declared types on the Python side: nothing checks a returned
  value against an expected type. This is the design adr:0023 described and adr:0025 retired for
  LotML's native target, so it is history for LotML, not a to-do.

## Testing and conformance

- **Differential, interpreter vs native**: `tests/differential.sh` runs every `*_test.px` under
  `tests/{language,interpreter,modules,types}` both ways and demands byte-identical output and
  exit 0, minus an `EXCLUDE` list of interpreter-only suites (`differential.sh:23-25`).
  `tests/run_all.sh` adds examples, name-hook parity, stdlib parity, negative checker suites,
  API/ABI freeze checks and parser fuzz.
- **Seeded parity fuzzing**: `tests/fuzz_gen.px` (written in Plix) emits checker-valid typed
  programs from an LCG seed (`fuzz_gen.px:1-60`); `tests/fuzz_parity.sh` builds and runs each both
  ways and keeps failing seeds (`fuzz_parity.sh:15-35`). CI runs 40 seeds (`.github/workflows/release.yml:49-50`).
- **Name-hook parity** (`tests/name_hook_parity.sh`, `tests/correctness-*-hook.px`): proves no
  function name triggers a special code path — added after the incident in Pitfalls.
- **Golden negative tests** with exact locations (`tests/errors/*.px`, `tests/syntax_errors.sh`).
- **API/ABI freeze**: machine-checked inventories with SHA-256 (`api_freeze.json`,
  `abi_freeze.json`, `src/abi_freeze.rs`).
- **CI**: everything above runs on `ubuntu-22.04` only; the Windows and macOS jobs build and run
  `plix --version` (`.github/workflows/release.yml:23, 46-50, 96-97`).

## Reusable for LotML

| Item | Path in repo | What it gives | LotML crate | License verdict | Effort |
|---|---|---|---|---|---|
| Toolchain fingerprint for a build cache | `build.rs:57-87, 161-180` | key = hash(compiler + runtime sources, version, target, rustc, lockfile), so a new compiler never reuses old artifacts | `lotml-llvm` (`driver.rs`) for a cached runtime object | MIT — adapt with attribution | S |
| Prebuilt runtime shipped inside the compiler | `build.rs:19-159`, `codegen.rs:3983-3998` | runtime compiled once, not per program | `lotml-runtime` + `lotml-llvm/driver.rs` | idea only: LotML's runtime is C compiled by the user's `clang`, so cache the object per toolchain rather than embed it | S–M |
| Seeded checker-valid program generator + run/build diff | `tests/fuzz_gen.px`, `tests/fuzz_parity.sh` | random typed programs diffed across both targets; failing seeds kept | `lotml-llvm/tests/` or `harness/` | MIT — idea (it is Plix source; rewrite in Rust/Python for LotML syntax) | M |
| Name-hook parity corpus | `tests/name_hook_parity.sh`, `tests/correctness-fib-hook.px` | guards benchmarks against special-cased names | benchmarks in `harness/` | idea | S |
| Cranelift AOT emitter, Windows link fallback, pyffi loader | `src/codegen.rs`, `rt/src/pyffi.rs` | — | — | nothing to take: Cranelift rejected by adr:0021/0025, native CPython retired by adr:0025, linking already delegated to `clang` | — |
| ABI freeze table | `src/abi_freeze.rs`, `abi_freeze.json` | — | — | nothing: `compiler/crates/lotml-runtime/src/abi.rs` derives signatures from `lotml.h`, which cannot drift | — |

## Ideas and optimizations worth adopting

1. **Compile the C runtime once per toolchain and flags, not once per program.** LotML's driver
   passes `lotml.c` to `clang` on every build (`compiler/crates/lotml-llvm/src/driver.rs:164`).
   Measured on the dev machine (clang 23.1.3, Windows, best-of-3 wall clock, outside the clones):
   `clang -c lotml.c` takes ~250 ms at `-O0 -g` and ~915 ms at `-O2`; compiling the 2.6 KB
   `tests/add.ll` takes ~55–80 ms; linking a trivial object ~95 ms (`clang` drives `lld-link`).
   The runtime is therefore ~60% of a small debug build and ~80% of a small release build. Plix
   avoids the cost by shipping a prebuilt runtime archive. For LotML: cache `lotml.o` per key in a
   per-user cache directory, key = runtime source hash (`lotml-runtime::FILES`), clang path +
   version, level, `-g`, `LT_COUNT_CELLS`, `LOTML_SANITIZE`, `-shared`/`-fPIC`/visibility, target.
   Nothing is lost at `-O2`: `x.ll` and `lotml.c` are already separate modules today (no `-flto`),
   so no cross-module inlining exists to forfeit. ADR touch: adr:0016 ("one translation unit") is
   unaffected — the runtime stays one TU — but adr:0025's consequence "compiled by `clang` with
   every program" becomes "compiled by `clang` once per toolchain"; **flag: say so in the spec or
   amend the wording**. Take plix's lessons with it: key on every input (plix keys only the main
   file, see Pitfalls), never a hard-coded path, and keep the cache user-private, since an object
   in it is linked into every executable.
2. **Run the LLVM parity tests on a Windows runner in CI.** Plix ships a calling-convention
   mismatch that, read from the source (not run), breaks native programs on x86_64 Windows — the
   very first call, `plix_rt_init(nglobals)`, passes its argument in the wrong register — while its docs list Windows as
   verified; its Windows job only runs `--version`. LotML's CI is the same shape: `ci.yml` runs
   only on `ubuntu-latest` (LotML `.github/workflows/ci.yml:16,45`), and the Windows release job checks only `lotml --version`
   (LotML `.github/workflows/release.yml:86-96`), while the runtime and driver carry Windows-only code
   (`compiler/crates/lotml-runtime/c/lotml.c:57-61` `<windows.h>`, `compiler/crates/lotml-llvm/src/driver.rs:61-65` installer lookup, the import library of
   `--shared`). No ADR conflict.
3. **A seeded generator of checker-valid LotML programs, diffed across `run` and `build`.** LotML's
   parity suite is hand-written programs (`compiler/crates/lotml-llvm/tests/programs/`, 9 files,
   plus per-area tests). Plix's generator is ~110 lines and stresses exactly what drifts between
   emitters: integer edge cases, shifts, `%` with negative operands, ternaries, calls
   (`fuzz_gen.px:35-60`). For LotML: generate over the subset the LLVM target supports, compare
   stdout, stderr and exit status against the Python target (adr:0025 makes it the reference), keep
   failing seeds as regression programs. Capture each side's exit status directly — plix's script
   reads the status of its output filter instead (`fuzz_parity.sh:19,26`). No ADR conflict.
4. **Do not add Cranelift as a fast debug backend.** The question was whether plix makes the case;
   it does not:
   - The ceiling of the gain is the `.ll` compile, ~55–80 ms for a small program, against ~250 ms
     of runtime compile and ~95 ms of link that Cranelift would not remove: plix itself still needs
     a system C linker, and LotML would still need `clang` for the C runtime. Idea 1 saves more,
     with no second emitter.
   - Plix's native speed is set by its runtime ABI, not its code generator: fib(30) native typed
     0.196 s vs CPython 0.123 s vs C 0.003 s, 10M-iteration loop 1.27 s vs C 0.042 s
     (`docs/comparison.md:46-69`) — boxed arguments, heap floats, TLS arena traffic and an
     out-of-line error-flag call after each operation. This is evidence for adr:0016 (no boxing,
     monomorphic values), not for a backend swap.
   - A second emitter is a second implementation of every construct (adr:0020's reason for one IR)
     and a second native result to hold in parity — what adr:0021 rejected and adr:0025 reaffirmed
     ("no Cranelift"). **Any reversal conflicts with adr:0021/0025 and needs a superseding ADR.**
   - It would become worth revisiting only if, after idea 1, a measured `lotml build` debug latency
     still dominates the agent loop and `lotml run` (CPython) cannot serve it. Then: Cranelift
     reading the same counted IR, `isa.default_call_conv()` instead of a hard-coded convention, and
     `clang` kept for the runtime and the link.
5. **Keep every backend off the syntax tree (adr:0020 is validated).** Plix's two AST walkers
   diverged in at least four places despite a parity battery (Pitfall 3). LotML already decided
   this; plix is a concrete counterexample worth citing in `docs/wiki/pages/transpilation-strategy.md`.
6. **Boundary guards: keep adr:0012's depth.** Plix guards only `int`/`float`/`bool`/nullable at
   typed slots; `str`, struct and container annotations are trusted at run time
   (`codegen.rs:633-643`). LotML's wrappers checking element types, records and variants are the
   sound version; plix shows the cheaper scalar-only design leaks. Also from plix: one runtime
   function owns each guard message so both targets print identical text (`codegen.rs:784-785`) —
   LotML's two runtimes (`lotml_rt.py`, `lotml.c`) duplicate CPython's messages by hand
   (`compiler/crates/lotml-runtime/c/lotml_text.c:348`); low value until a mismatch appears in parity.
7. **Benchmark integrity check.** After plix's name-based fast paths (Pitfall 4), it added a corpus
   proving no function name changes codegen. LotML's benchmarks compare against hand-written C;
   a cheap guard is to run each benchmark also with renamed functions and require identical output
   and similar time. Low impact; no ADR touched.

## Pitfalls seen

1. **Hard-coded System V calling convention.** `make_sig` always uses `CallConv::SystemV`
   (`codegen.rs:497`, also `jit.rs:187`), for compiled functions, imported runtime functions and the
   exported `main`. The runtime is `extern "C"` (`rt/src/heap.rs:692`, `value.rs:964`) and calls
   compiled closures as `extern "C" fn` (`value.rs:909`). On x86_64 Windows these disagree in
   argument registers (rdi/rsi/rdx vs rcx/rdx/r8), shadow space and callee-saved registers, so
   calls in both directions get wrong arguments. `abi_freeze.json` states "C ABI (extern "C") on all
   targets" and `docs/targets.md` lists Windows as verified by CI, but the Windows job only runs
   `--version` (`release.yml:96-97`). Lesson: take the convention from the target
   (`isa.default_call_conv()`), and test native output on every OS you ship.
2. **Object cache keyed on the main file only.** The key is `hash(src)` of the entry file
   (`codegen.rs:190-193`) while `build_object` also reads every imported `.px`
   (`codegen.rs:275-316`): edit a module, rebuild, get the old binary. The cache root is a
   hard-coded `/home/user/plix_ch` (`codegen.rs:185`), shared across users where writable. An
   earlier version also lacked the compiler fingerprint and served objects from older compilers
   (`CHANGELOG.md:216-224`). Lesson for idea 1: key on every input, per-user directory.
3. **Two walkers drift.** (a) Typed int overflow: native raises "integer overflow in typed int
   addition" (`codegen.rs:1292-1300`, `int_range_guard` 1382-1400); the interpreter's
   `strict_int_arith` promotes to float on overflow (`interp.rs:111-128`) even though the comment
   above its call says it "raises instead of promoting" (`interp.rs:736-737`) and `ast.rs:26-30`
   and `docs/typing.md:65-80` say both backends raise. (b) Scoping: native rejects a repeated local
   name in a function body because `resolve.rs` uses a flat namespace (`resolve.rs:17-18, 220-224`),
   while the interpreter gives every block its own scope (`interp.rs:408-412`) — programs that
   `run` may not `build`. (c) A raw local read before assignment is `0` natively and `null`
   interpreted (`docs/typing.md:109-111`). (d) `Result`/`Option`/nullable programs are
   interpreter-only (`differential.sh:23-25`). The docs also contradict themselves on `Option`
   (`docs/typing.md:32` vs `:144`).
4. **Benchmark gaming by name.** The native backend replaced the body of any one-argument function
   whose name contained `fib` with an iterative Fibonacci, and any zero-argument `run` with
   `return 465` (`CHANGELOG.md:189-205`; note left at `codegen.rs:166-172`). Published numbers
   were measured with those hooks.
5. **Facades.** The "JIT" never executes code (`jit.rs:465-469`); the guard-parity section of
   `run_all.sh` globs `tests/guards/g*.px` under `nullglob` (`run_all.sh:32-33`), a directory that
   does not exist, so it passes with zero tests; `tests/test_strict.px:6-11` uses `try`/`catch`,
   which the language does not have. DeepWiki repeats the JIT claims — map, not evidence.
6. **Per-operation runtime overhead dominates.** Each heap variable read is retain + TLS `RefCell`
   borrow + `Vec` push (`heap.rs:587-601`); each fallible op is followed by a non-inlined
   `plix_err_flag()` call and a branch (`codegen.rs:693-701`); each call pushes and pops a frame
   arena (`codegen.rs:3321`, `finish_body` 3253-3258); typed multiply checks overflow with an
   `sdiv` (`codegen.rs:1303-1346`). Result: slower than CPython on calls (Idea 4).
7. **Python bridge correctness.** After `PyLong_AsLongLong` the code checks Plix's own error flag,
   not Python's, so a Python int beyond 64 bits arrives as `-1` and leaves a Python exception set
   (`pyffi.rs:452-460`). `type_of` reads `ob_type` at a fixed offset (`pyffi.rs:293-298`), which
   the free-threaded CPython build lays out differently. On Windows, `LoadLibraryA` of bare
   `python3XY.dll` names searches the current directory (`pyffi.rs:132-140`). adr:0012 already
   requires range checks at the boundary; these are the failure modes it prevents.
8. **Reference counting without cycle collection, with shallow copies.** Cycles leak; `b_sort_by`
   and map iteration copy `V`s without retaining them, making mutation during iteration undefined
   (`docs/memory_limitations.md` §2). Freeing nested containers recurses (`heap.rs:172-230`).
9. **Build-script fragility around the embedded runtime.** Cargo's rlib and staticlib outputs race,
   so `build.rs` compares mtimes and may run a nested `cargo build` into a scratch target dir to
   avoid a lock deadlock (`build.rs:107-158`); every link writes a ~23 MB archive to temp
   (`codegen.rs:4025-4027`).
