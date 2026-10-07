# pon — Python 3.14 compiled through one IR by Cranelift, JIT for `pon run`, AoT objects linked into an executable for `pon build`

can1357/pon (per the DeepWiki export) · **no license**: no `LICENSE`/`COPYING` file, no `license`
key in any `Cargo.toml`, no SPDX header. Ideas only, never code · last commit 2026-07-07 (shallow
clone, one commit) · Rust nightly-2026-04-29, edition 2024, Cranelift `=0.133.1`, ruff parser tag
`0.14.0` (`README.md:116-126`). Paths below are relative to `research/prior-art/repos/pon/` unless they
start with `compiler/`, `harness/`, `docs/` or `specs/`, which are LotML's.

Size, in lines of Rust: `pon-runtime` 210.7k (native stdlib modules 121k, `abi/` + `abi.rs` 28.9k,
`types/` 23.2k, C-API shim 20.9k) plus 6.2k lines of C headers (`pon-runtime/include/Python.h`
and others); `pon` (CLI and a uv-style package manager) 18.0k; `pon-codegen` 12.3k; `pon-ir`
10.5k; `pon-conformance` 5.3k, plus a 14.6k-line `.py` corpus and a vendored CPython 3.14.0
`Lib/`; `pon-gc` 2.5k; `pon-aot` 1.9k; `pon-jit` 1.7k; `pon-abi` 0.1k; `pon-aot-dynamic` 0.1k.

Maturity: research grade, Linux and macOS only. The committed floors say 244 corpus modules
pass byte-exact against CPython under the JIT (`conformance-floor.json`) and 233 pass as AoT
executables (`aot-parity-floor.json`). The floor for CPython's own test suite is 0
(`conformance-full-floor.json`). The README's own counts are already stale: 209 and 172 at
`README.md:132`.

## Architecture

The pipeline, stage by stage:

| Stage | Crate / module | Key types |
|---|---|---|
| Parse | `pon-ir/src/parse.rs:17-25`: ruff `parse` pinned to `PythonVersion::PY314` | ruff `ModModule` |
| Scope analysis | `pon-ir/src/lower/scope.rs` (1.7k lines) | `NameClass::{Local, Global, Free, Cell, Builtin}` (`scope.rs:32-43`) |
| AST → IR | `pon-ir/src/lower.rs` plus one file per construct family in `lower/` | `LowerError::{Parse, Unsupported{feature, span}, Internal}` (`lower.rs:96-131`) |
| Generator transform | `pon-ir/src/lower/generator.rs:293` `split_suspend_points`, `:461` `localize_cross_block_values` | `Terminator::Suspend` |
| IR | `pon-ir/src/ir.rs` | `Module{functions, main, names}` (`:15-23`), `Function` (`:85-108`), `Block{id, insts, term}` (`:152-159`), `Inst{result, kind, feedback_slot, inferred_type, static_type, line}` (`:185-198`) |
| Codegen | `pon-codegen`: `baseline.rs` (tier 0, all values boxed) and `optimizing.rs` (tier 1, and AoT `--opt`) | `compile_ir_module<M: cranelift_module::Module>` (`lib.rs:54-86`) |
| JIT | `pon-jit/src/lib.rs` `JitEngine` (`:32-37`); tier-up in `tierup.rs` | `TierUpDriver`, `Tier1Compilation` |
| AoT | `pon-aot/src/lib.rs:39-207` `build`, reachability in `reachable.rs`, linking in `link.rs` | `CompileUnit`, `ReachabilityReport` |
| Runtime | `pon-runtime`, built both as an `rlib` and a `staticlib` (`pon-runtime/Cargo.toml:8-9`); GC in `pon-gc` | `HELPERS: &[HelperDecl]` (`pon-runtime/src/abi.rs:901-1034`) |

### How `run` and `build` share one IR and one codegen

The whole trick is one generic parameter. `compile_ir_module` and `compile_optimized_ir_module`
take `M: cranelift_module::Module` (`pon-codegen/src/lib.rs:54-128`), so the same lowering writes
into a `JITModule` or an `ObjectModule`. Everything the generated code needs from outside is
reached in one of two ways:

- **Runtime helpers.** Each is declared by symbol name with `Linkage::Import`
  (`pon-codegen/src/helpers.rs:1180-1313`).
- **Module data.** String bytes, per-function feedback cells and name tables use
  `declare_anonymous_data` (`baseline.rs:1962`, `:1979-2000`).

Neither depends on the backend. Only two things change with `CompileMode` (`lib.rs:37-46`):

- the symbol of the module body (`__pon_module_body`, local, in AoT; `lib.rs:147-150`);
- whether the codegen asks for stack maps (`lib.rs:77-80`).

Where the two paths differ:

| Concern | JIT (`pon run`) | AoT (`pon build`) |
|---|---|---|
| ISA flags | `make_isa(OptLevel::None, pic=false)` on the host (`pon-codegen/src/isa.rs:43-72`); tier 1 uses `OptLevel::Speed` (`pon-jit/src/tierup.rs:580`) | `opt_level=none`, `is_pic=true`, `tls_model` per format (`elf_gd`/`macho`/`coff`), `enable_llvm_abi_extensions`, inline stack probes on x86-64, AArch64 and RISC-V (`pon-aot/src/isa.rs:32-74`); other triples go through `isa::lookup` (`:16-28`) |
| Shared flags | `preserve_frame_pointers=true`, `use_colocated_libcalls=false`, so a helper call never assumes a near target (`isa.rs:37-59`) | same |
| Helper symbol resolution | `JITBuilder::symbol(helper.symbol, helper.address)` for every `HELPERS` row, plus the `pon_current_line` data cell and three threading shims (`pon-jit/src/lib.rs:112-130`, `:324-340`) | left as imports; the system linker resolves them against the `#[no_mangle]` exports of `libpon_runtime.a` |
| Python function symbols | `__pon_fn_{index}`, local | the same, plus an exported zero-argument wrapper per module body (`pon-aot/src/entry.rs:157-188`) |
| Runtime name ids baked into code | interned in the live process (`NameMap::from_ir_module`, `baseline.rs:51-61`) | the build process's whole interner is replayed at start-up by a generated `pon_aot_init_names` calling `pon_aot_intern_name` once per name (`entry.rs:81-117`, `pon-aot/src/lib.rs:180-201`) |
| Imports | compiled lazily from source at run time by a loader hook (`pon/src/run.rs:200-203`) | the static import closure is computed at build time (`reachable.rs:248-401`): one object file per module, announced by a generated `pon_aot_init_modules` registrar (`entry.rs:197-251`) |
| GC roots in generated frames | tier 0 publishes Cranelift user stack maps (`pon-jit/src/lib.rs:143-202`); tier 1 does not (`tierup.rs:530-533`) | none: conservative stack scanning only (`pon-codegen/src/lib.rs:77-80`) |
| Tiering | hotness counters, a background compile thread, OSR | none, yet the OSR and tier-up probes are still emitted and called (see Pitfalls) |
| Entry | `boot_runtime` + `JitEngine::run` in Rust (`pon/src/run.rs:200-257`) | an object-defined `main` → runtime `pon_aot_entry` → generated `pon_module_main` (`entry.rs:27-72`, `pon-runtime/src/aot_entry.rs:52-132`) |
| `eval`/`exec`/`compile` | the JIT compiles them in-process | refused at build time unless `--allow-dynamic`, which links `libpon_aot_dynamic.a` (the runtime plus JIT and parser); `main` then installs hooks first (`pon-aot-dynamic/src/lib.rs:30-34`) |

Note: the JIT does not call the shared entry point. `JitEngine::compile` repeats the declare,
lower and define loop so that it can collect stack maps while `ctx` still holds the compiled
code (`pon-jit/src/lib.rs:137-217`). Only AoT uses `compile_ir_module`.

**Where JIT/AoT parity actually breaks.** `aot-parity.json` records 233 aot-pass, 8 aot-fail,
3 aot-refused and 0 aot-error:

- The 3 refusals are `eval`/`exec` reached statically.
- The 8 failures are `type_dict_descriptors`, `sysconfig_build_config`, `ast_parse`,
  `importlib_source_finder`, `import_blocked_none`, `future_annotations_pep563`,
  `pep562_module_getattr` and `relative_import_function_scope`.

Their details read "embedded module 'importlib' returned NULL" and "requires the host JIT loader
for unsupported statement". These are import machinery and runtime environment, not code
generation: sharing the codegen removed codegen divergence, and what remains is everything around
it. LotML's two backends do not share a codegen (ADR 0025), so LotML gets both kinds.

## Frontend

- **Parser.** ruff's hand-written recursive-descent parser. Indentation is handled by ruff's
  lexer.
- **No error recovery is used.** `parse()` errors become one `LowerError::Parse`
  (`pon-ir/src/parse.rs:17-25`), and lowering stops at the first construct it cannot handle.
- **No incremental or salsa layer.** Every `pon run` re-parses and re-lowers. Every import does
  the same through the source loader.
- **AST** is ruff's own.
- **Refusing unsupported code.** Unsupported code fails with
  `LowerError::Unsupported{feature, span}`, and the harness classifies any non-zero exit whose
  stderr says "unsupported" as `unsupported` rather than `fail` (`pon-conformance/src/suite.rs:480-494`).
  The convention is cheap and makes "not yet" measurable apart from "wrong".
- **Dynamic sinks.** A separate source scan finds the dynamic sinks: `DynamicCodeKind::{Eval,
  Exec, Compile}` (`pon-ir/src/lower.rs:55-80`) through `scan_dynamic_sinks_source`, which AoT
  reachability calls before lowering (`pon-aot/src/reachable.rs:290-314`).
- **The refusal names the place and the remedy.** For example, "`eval` reached statically at
  file:1:7 … rebuild with --allow-dynamic" (`aot-parity.json`, refusal buckets). LotML's
  refusal of a Python import under `build` (ADR 0025) is the same shape.

Compared with LotML: `lotml-syntax` has a tolerant parser, and `lotml-db` adds salsa
incrementality. pon has nothing here LotML lacks. ruff's parser is studied separately (the
`ruff` clone).

## Semantics and types

pon has no checker: Python semantics are carried by the runtime helpers, and the IR's values
are all boxed `*mut PyObject` (`pon-ir/src/ir.rs:1-7`). Types exist only as speculation metadata
for the optimizing tier:

- **The lattice.** `Type::{Bottom, Int, IntI64, Float, Bool, Str, ExactClass(u32), Object}`.
  Join widens any two different types to `Object`, except that `IntI64 ⊔ Int = Int`
  (`pon-ir/src/types.rs:10-46`). Only `IntI64` and `Float` are unboxable (`:52-54`).
- **Two types per instruction**: `inferred_type` is speculative and must be guarded,
  `static_type` is a sound bound; `Inst::new` starts them at `Bottom` and `Object`
  (`ir.rs:207-216`).
- **`infer_module_types`** (`pon-codegen/src/infer.rs:15`) is one forward pass in block order,
  seeded by annotations (`int` maps to `IntI64`, `annotations.rs:241-242`), with every
  unannotated parameter seeded `IntI64` (`infer.rs:82-88`); `+ - * // %` on two `IntI64` yields
  `IntI64` (`infer.rs:183-195`).
- **AoT `--opt` strips annotations and re-lowers.** It re-parses each unit, reads its
  annotations, strips them, lowers again and infers (`pon-aot/src/lib.rs:234-247`). The JIT's
  tier 1 runs the same inference with empty annotations (`pon-jit/src/tierup.rs:402`).
- **No generics or monomorphization.** Python has none to specialize. Classes are runtime
  objects built by `pon_build_class_*`.
- **Dynamic features** are mostly implemented at run time rather than refused: `match`,
  generators, `async`, `except*`, PEP 649 lazy annotations, PEP 695 aliases, t-strings
  (`ir.rs:445-634`). The only build-time refusal is dynamic code in AoT.

None of this maps onto LotML's checker (`lotml-check`). The useful contrast is that pon has to
*guess* `int` and guard it, where LotML knows the type.

## IR and passes

One IR level, a CFG in SSA form over boxed values:

- `Block{id, insts, term}`; there are no block parameters (`ir.rs:152-159`).
- Locals live in slots (`LoadLocal`/`StoreLocal`, `ir.rs:297-301`).
- About 90 `InstKind`s map nearly 1:1 onto `pon_*` helpers (`ir.rs:247-635`).
- Terminators: `Return`, `Jump`, `Branch`, `CondBranch`, `ForLoop`, `Suspend`, `RaiseTerm`,
  `Unreachable` (`:647-692`).
- Names are module-local `NameId`s into `Module::names` (`:15-23`).
- Each `Inst` carries the 1-based line of its statement (`:176-198`).
- There is **no IR verifier**: malformed IR surfaces at codegen as
  `CodegenError::ValueNotDefined`, `LocalUsedBeforeDefinition` and similar (`baseline.rs:96-120`).
  LotML has `lotml-ir/src/verify.rs`.

The passes:

- **Scope analysis.** LEGB classification into local, global, free, cell or builtin
  (`lower/scope.rs`).
- **Desugar.** An explicit identity pass kept as a slot (`pon-ir/src/desugar.rs:15-18`).
- **Generator state machine.** `Yield` and `YieldFrom` become `Suspend` splits. Values crossing
  a suspend are spilled into frame slots (`lower/generator.rs:293`, `:461`).
- **Exception-handler stack dataflow.** A worklist proves that every block has one static handler
  stack; an inconsistent join is an error (`baseline.rs:644-659`).
- **OSR liveness.** Gen/kill sets, then the live-ins at a loop header (`baseline.rs:2213`).
- **Temporary spill windows.** Values live across a re-entrant helper call are stored to explicit
  slots for the conservative scan (`baseline/spill.rs:1-20`).
- **Local type inference.** Metadata only (`infer.rs`).
- **Typed-region finder.** The largest single-entry region whose instructions are all in the
  fast-path subset (`region.rs:74-134`).
- **Optimizing plan.** Entry guards, the unboxed fast path, a cold boxed twin and side exits
  (`optimizing.rs:34-191`). Only `IntI64` regions compile (`can_compile_plan`, `:139-162`), and
  generators are excluded (`:108-114`).
- **Back-edge detection.** A jump to a block whose id is not greater than the current one
  (`baseline/control.rs:182`). It relies on layout order, not on dominators.

## Backend and toolchain

- **Function ABI.** Every Python function compiles to `(argv: *mut *mut PyObject, argc) ->
  *mut PyObject` (`baseline.rs:397-404`). The parameter locals load from `argv` (`:473-481`).
  Generators compile to two functions:
  - a stub with that ABI, which allocates a `GenFrame`;
  - a resume body `(frame) -> obj`, which dispatches on `resume_state` through `br_table`
    (`baseline/gen.rs:1-21`).
- **Errors** are a NULL sentinel. After every object-returning helper, the code branches to a
  shared cold `exception_exit` block, which returns NULL (`baseline.rs:2151-2202`, `:458-459`).
  Status helpers return `-1` (`pon-runtime/src/abi.rs:4478-4489`). Every helper body is wrapped in
  `catch_unwind`, which turns a Rust panic into NULL plus a pending error (`abi.rs:4491-4503`).
- **Source lines** are written with a direct `i32` store to the exported process-global cell
  `pon_current_line`, only when the statement line changes. Three instructions, no call
  (`baseline.rs:2002-2037`, `abi.rs:490-501`).
- **Object emission.** `cranelift-object` writes one `.o` per reachability unit
  (`pon-aot/src/lib.rs:91-150`) and one for the entry module. Per-function sections are on,
  except on COFF (`object_module.rs:14-19`). Mach-O objects get a stamped `LC_BUILD_VERSION`
  (`buildver.rs:11-21`). There is no debug info: no DWARF, no CodeView.
- **Linking.** `$CC`, or `cc`, gets the objects, the runtime archive and `-o` (`link.rs:24-26`).
  - ELF adds `-lpthread -ldl -lm -lpanel -lncurses -llzma`.
  - Mach-O adds `-lpanel -lncurses -liconv -llzma` and two frameworks.
  - COFF adds nothing (`link.rs:28-41`).

  The archive is `libpon_runtime.a`, or `libpon_aot_dynamic.a` for `--allow-dynamic`
  (`link.rs:11-12`). It is located in this order (`link.rs:64-111`):
  1. `PON_RUNTIME_LIB` or `PON_AOT_DYNAMIC_LIB`;
  2. `<exe>/../lib/`;
  3. beside the executable;
  4. `target/{$PROFILE,debug,release}/`.

  A failure lists every path it tried.
- **Windows: not supported in practice.**
  - The archive name is the Unix one, and the default linker is `cc`.
  - There is no MSVC, `lld-link` or clang discovery.
  - Thirty-three files under `pon-runtime/src` call `libc::` (`grp`, `pwd`, `termios`,
    `_posixsubprocess`, `mmap`, `fork`, …) with only two `#[cfg(unix)]` guards
    (`native/os.rs:802`, `:2299`).
  - The runtime also uses x86-64 and AArch64 naked asm (`abi.rs:4846-4865`).
  - CI is `ubuntu-latest` only (`.github/workflows/conformance.yml`).

  LotML's `driver::find` is already ahead (`compiler/crates/lotml-llvm/src/driver.rs:36-64`): it
  tries `LOTML_CLANG`, then `PATH`, then `%ProgramFiles%\LLVM\bin\clang.exe`, and checks the
  version.
- **Cross-compilation is half-built.** `--target <triple>` selects the Cranelift ISA, but linking
  still runs the host `cc` against the host-built archive (`link.rs:24`, `:64`).
- **Shared libraries.** None. The runtime archive has undefined references to
  `pon_module_main`, `pon_aot_init_names` and `pon_aot_init_modules` (`aot_entry.rs:14-18`), so
  it cannot even link without a generated `main`. Contrast LotML's `build --shared` (ADR 0024).
- **Optimization.** AoT is `opt_level=none` even with `--opt`; `--opt` only switches to the
  typed lowering (`pon-aot/src/isa.rs:34`).

## Runtime

- **Object header.** `{ob_type: *const PyType, gc_meta: GcMeta(usize)}`, two words, no
  reference count. CPython's layout "omitting reference-count storage: ownership is delegated to
  `pon-gc`" (`pon-runtime/src/object.rs:1-6`, `:17-62`).
  - Small integers are tagged in bit 0, 63-bit payload (`tag.rs:1-40`).
  - Big integers are `num-bigint` behind `PyLong`.
- **The helper ABI, in one table.**
  - `HelperDecl{symbol, address, params: &'static [AbiTy], ret}` (`abi.rs:772-781`).
  - `AbiTy` has 20 C-level shapes (`:726-767`).
  - `HELPERS` holds 172 rows (`:901-1034`).

  Codegen declares imports only from this table and panics if a symbol is missing
  (`pon-codegen/src/helpers.rs:1306-1313`); the JIT binds address by address. The table cannot
  drift from the exported functions, because each row takes the function's address. LotML does
  the equivalent by parsing `lotml.h` (`compiler/crates/lotml-runtime/src/abi.rs:1-3`, `:73-80`),
  so there is nothing to borrow. pon also keeps a *second*, descriptive table in codegen,
  `PHASE_B_HELPERS` (`helpers.rs:508`), which can drift.
- **Builtins and native modules.** `LoadBuiltin(name)` calls `pon_load_builtin`, which re-checks
  the module's globals first, because a module may shadow a builtin
  (`pon-runtime/src/abi/builtins.rs:8-27`). Native stdlib modules are one sorted, append-only
  table, `NATIVE_MODULES: &[(&str, fn() -> Result<*mut PyObject, String>)]`
  (`native/mod.rs:1-10`, `:93`); a fixed set is eager and the rest are lazy.
- **Calls.** `pon_call` is a naked-asm shim that captures the caller's SP
  (`abi.rs:4846-4865`). It then dispatches (`:4869-4950`), and for a compiled function enters
  through `call_code_pointer` (`:4986-5045`). Each call costs `catch_unwind`, a thread-local
  push of `(callee, argv, argc, sp)` for GC rooting (`:4632-4637`), `maybe_auto_collect`, an init
  check and the runtime lock, a type switch, a side-table record lookup, an arity check, an atomic
  hotness bump (`:5030`), a reload of the `entry` dispatch cell, a recursion guard and a
  current-function thread-local push before the indirect call. This is the dynamic-Python tax,
  and AoT executables pay all of it.
- **Inline caches.** One 32-byte `FeedbackCell` per specializable site, as a seqlock with
  attribute, global and call IC variants (`feedback.rs:1-19`, `:79-110`). Each compiled function
  gets one writable zero-initialized data object (`baseline.rs:446-453`, `:1979-2000`). The cell
  address is passed *to the helper*, which checks it on entry; for example `pon_load_global`
  validates dict version and identity (`abi.rs:6259-6270`). The fast path is not inlined into
  generated code. Because the cells are object-file data, ICs work unchanged in AoT.
- **GC.**
  - The heap: 64 KiB aligned spans, 16-byte size classes up to 1 KiB, span-granular marking in
    the style of Go's Green Tea (`pon-gc/src/lib.rs:1-20`), and a stop-the-world handshake
    (`pon-gc/src/handshake.rs`).
  - Generated code calls `pon_safepoint_poll` at function entry and on every back-edge
    (`baseline.rs:2133-2149`, `control.rs:171-213`).
  - It calls an out-of-line write barrier for every argument stored to an argv slot
    (`baseline.rs:2117-2131`).
  - Frame pointers are forced for the whole workspace (`.cargo/config.toml`).
- **Strings and literals.** Every evaluation of a `str` literal calls `pon_build_string` and
  allocates (`baseline/strings.rs:19-48`). Every `int` literal is a `pon_const_int` call
  (`abi.rs:4563-4566`).
- **CPython interop.** A C-API emulation layer for extensions: `pon-runtime/src/capi/`, 20.9k
  lines, plus `include/Python.h`. `Py_INCREF`/`Py_DECREF` *pin* objects rather than count them
  (`include/Python.h:1054`).
- **AoT start-up order** (`aot_entry.rs:64-132`): name replay, module registration,
  `pon_runtime_init`, stack-base capture, `sys.argv`, `__main__` install, `pon_module_main`, the
  uncaught report, atexit callbacks, flushing std streams; exit status 1 on any failure. The
  generated `main` is a one-call trampoline (`entry.rs:27-72`).

### Why not reference counting, and what that cost

The repository never states the reason. The only statements are:

- "Memory is managed by a Green Tea garbage collector instead of reference counting"
  (`README.md:7`);
- "CPython's heap object layout minus the refcount header" (`README.md:56`).

The design notes the code cites (`plans/pon-pin-J0{1..7}-*.md`, eight references) are not in the
clone. Two things in the code imply the motive:

- a free-threaded runtime (the `ft-stress` suite, `tests/ft/`), where counts would have to be
  atomic;
- a C-API that only needs *pins*.

What the code shows is the bill:

- **No precise stack maps in AoT.** Executables fall back to conservative scanning
  (`pon-codegen/src/lib.rs:77-80`), which needs frame pointers everywhere.
- **Dead values pin garbage.** The fixes, each added after a corpus failure:
  - every argv stack array is zeroed after its call (`baseline.rs:2039-2081`, after AoT
    `weakref_basic`);
  - every named local is mirrored into a shadow frame slot (`baseline.rs:265-274`);
  - temporaries live across re-entrant calls get spill windows (`baseline/spill.rs:1-20`);
  - stack maps were added to the JIT's tier 0 (`baseline.rs:625-641`, after `weakref_dicts`).
- **Rust helpers hide roots.** Helpers keeping argv in a `Vec` need thread-local root
  registries, `ACTIVE_CALL_OPERANDS` and `SCOPED_ROOT_SOURCES` (`abi.rs:4632-4641`).
- **A write barrier per argument store**, as an out-of-line call (`baseline.rs:2127-2130`).
- **Divergences pon cannot fix.** The divergence ledger's closed taxonomy exists to excuse them:
  `refcount-observability`, `del-timing`, `weakref-timing`
  (`pon-conformance/divergence-ledger.toml:15-27`).

For LotML this confirms ADR 0003 and ADR 0008 rather than challenging them:

- With counting there are no roots to find, so no stack maps, shadow slots or barriers, and
  frees happen at last use.
- The no-GIL concern that pushed pon away from counting is what ADR 0008 already handles: counts
  are non-atomic within a task, and a value handed to another task is marked shared once
  (`compiler/crates/lotml-runtime/c/lotml.h:105-125`).

Nothing in pon argues for a collector.

## Testing and conformance

The contract: byte-identical output to CPython 3.14.0. These are the exact mechanics, because
LotML's Python-target versus LLVM-target parity is the same problem.

- **Corpus.** `pon-conformance/corpus/MANIFEST` lists 244 workspace-relative paths, one per line,
  with `#` comments (`suite.rs:238-255`); the JIT floor of 244 is the whole manifest. The files live in `pon-conformance/corpus/cpython/`
  (262 files) and `corpus/`. Corpus files are immutable once landed: "new coverage is a new
  module, verified byte-identical against python3.14 before it enters the manifest"
  (`README.md:103`).
- **One run.** Both sides run with `TZ=UTC` and `PYTHONHASHSEED=0` (`suite.rs:369-390`).
  - `RunResult{stdout, stderr, exit}` is compared with `==`, so **stderr must match byte for
    byte as well**, tracebacks included (`suite.rs:42-47`, `:173`).
  - A non-zero exit whose stderr contains "unsupported" is classified `Unsupported`, not `Fail`
    (`:480-494`).
  - The `pon` under test is `cargo build -p pon` at debug (`suite.rs:313-337`).
- **Floors.** Three committed files share one format, `{"cpython_tag", "passing_modules": [...],
  "min_pass_count"}` (`ratchet.rs:15-20`): `conformance-floor.json` (JIT corpus, 244),
  `aot-parity-floor.json` (AoT corpus, 233), `conformance-full-floor.json` (CPython `Lib/test`, 0).
- **Floor commands.**
  - `--check-floor` fails when any floor module no longer passes, *or* when the pass count falls
    below `min_pass_count` (`ratchet.rs:76-113`).
  - `--update-floor` rewrites the file from this run's passing set, sorted and deduplicated
    (`:115-140`).
  - `--diff-floor` prints `floor-diff regressed <m>` and `floor-diff progressed <m>` lines
    (`:146-180`).
  - Floor operations are refused on a sharded or filtered run, so a partial run can never lower
    a floor (`pon-conformance/src/main.rs:72-77`).
- **AoT parity** (`--mode aot --suite aot-parity`, `aot.rs:366-531`). For every manifest module:
  1. run `python3.14`;
  2. `pon build <m> -o <exe>`, killed after `PON_AOT_TIMEOUT_SECS` (default 60;
     `aot.rs:24-28`, `:603-647`);
  3. run the executable with `env_clear()`, `PATH=/usr/bin:/bin` and `HOME` only, to prove it
     does not need `pon` (`aot.rs:649-659`).

  Each module is classified `aot-pass`, `aot-fail`, `aot-refused` (a build stderr saying
  "unsupported") or `aot-error` (a build failure, a timeout, or a Rust panic in the executable,
  `aot.rs:543-546`). Refusals are bucketed by message, sorted by count (`aot.rs:125-152`). The
  report goes to the committed `aot-parity.json` (`aot.rs:154-231`, `:396-400`).
  - Records that fail do **not** fail the process; only the floor gates
    (`main.rs:223-224`: "aot-fail/aot-error records are bugs to report, not process-failing
    conditions").
  - The older `cpython-aot-subset` suite checks three ways, `pon run` against CPython, AoT
    against CPython and AoT against `pon run` (`aot.rs:333-342`), but its floor is hard-coded to
    0 in source (`aot.rs:47-48`).
- **Gate script** (`scripts/gate.sh:25-37`). It captures each exit status directly, never
  through a pipe, and only its output counts as a gate claim (`README.md:90`).
  - `fast`: build, workspace tests, the JIT floor, the AoT floor, `ft-stress`.
  - `full` adds the `cpython-full` floor, the bench gate, **the JIT floor again with
    `PON_TIER0_ONLY=1`** (so tier 1 cannot hide a tier-0 bug), a 200-case fuzz and the
    package-manager end-to-end test.
  - CI runs `fast`, a 50-case fuzz and the `cpython-full` floor on Ubuntu
    (`.github/workflows/conformance.yml`).
- **Bench gate.** Each benchmark runs at tier 0 and at tier 1. Their outputs must be equal before
  the speed-up is reported (`main.rs:512-530`).
- **Fuzz** (`fuzz.rs`).
  - **Generation.** Programs are deterministic per `(seed, case)`, from SplitMix64
    (`:447-463`, `:592-614`). Ten feature templates (arithmetic past 2^64, strings, control flow,
    closures, descriptors, generators with `send`/`throw`/`close`, `try`/`except`/`finally`,
    `match`, `del`, comprehensions) are emitted between `# PONFUZZ BEGIN <f>` and
    `# PONFUZZ END <f>` markers (`:486-590`).
  - **Isolation.** Each side gets a scrubbed environment, its own scratch directory and its own
    `TMPDIR`, plus rlimits, a process group and a 5 s kill (`:280-304`, `:732-778`).
  - **Divergence** is defined as differing stdout, a differing *stderr class*, or a differing
    exit (`:131-135`). The stderr class is the last exception class named in it, "unsupported",
    or the first word (`:616-643`); traceback text never counts.
  - **Minimizer.** It deletes one marked chunk at a time, re-runs, keeps the deletion while the
    divergence persists, and restarts from the first chunk (`:361-384`). Repros are saved as
    `.py` and `.min.py` with both sides' stdout and stderr and a summary (`:317-337`).
- **cpython-full.** A generated `unittest` driver, the same bytes on both sides, records an
  outcome per test id; the runner compares the outcome vectors (`full.rs:1-60`).
  - Exclusions, `exclusions.toml`, are capped at 40 entries with a closed reason taxonomy.
  - Divergences, `divergence-ledger.toml`, are capped at 25, and an entry must cover every
    differing test id and carry an `approved_by`. Agents may not self-approve in the change that
    makes a test pass (`divergence-ledger.toml:1-45`).

## Reusable for LotML

No license, so every row is **idea only**. Nothing can be copied or adapted.

| Item | Path in pon | What it gives | LotML home | Verdict | Effort |
|---|---|---|---|---|---|
| Ratcheted floor file + check/update/diff | `pon-conformance/src/ratchet.rs:15-180`, `main.rs:72-77` | a parity suite that cannot regress silently and records progress | `harness/lotml_harness/experiments/parity.py` (today one all-or-nothing `passed`, `:93-95`) | idea only | S |
| Outcome classes + refusal buckets + committed results JSON | `pon-conformance/src/aot.rs:51-231` | `refused`/`error` counted apart from `fail`; the backlog grouped by refusal message | same file; its verdicts `same/differs/refused/not compiled/no report` already exist (`parity.py:36-42`) | idea only | S |
| Template fuzzer with chunk markers + greedy minimizer + stderr-class comparison | `pon-conformance/src/fuzz.rs:131-135`, `:361-384`, `:447-643` | differential fuzzing Python-target vs LLVM-target, with minimized repros | new `harness/lotml_harness/experiments/fuzz.py` | idea only | M |
| Run the built executable with a scrubbed environment | `pon-conformance/src/aot.rs:649-659` | proves `lotml build` output needs nothing from the toolchain on `PATH` | `compiler/crates/lotml-llvm/tests/common/mod.rs:61-95` | idea only | S |
| Tier-0-only rerun of the floor | `scripts/gate.sh:33` | the slow, simple configuration stays correct on its own | the harness parity run at `-O0` as well as `-O2` (R5.1 runs `-O2` only) | idea only | S |
| Runtime archive locator (env var, `exe/../lib`, beside the exe, `target/`), listing what it tried | `pon-aot/src/link.rs:64-111` | a prebuilt runtime found without configuration | `compiler/crates/lotml-llvm/src/driver.rs` if the runtime object is cached | idea only | S |
| Divergence ledger: capped, closed taxonomy, approval field | `pon-conformance/divergence-ledger.toml:1-45` | known Python/LLVM differences recorded instead of special-cased | `specs/llvm-parity/` plus a harness file | idea only | S |
| `pon.ir`/`pon.clif`/`pon.asm` introspection | `pon-jit/src/inspect.rs:1-40`, `pon-ir/src/print.rs:1-30` | show a function's IR and machine code on demand | a `lotml` command printing a symbol's IR (`lotml-ir/src/text.rs`) and `.ll`; I found no subcommand that prints either | idea only | S |
| `HELPERS` single-source table | `pon-runtime/src/abi.rs:772-1034` | — | LotML already parses `lotml.h` (`lotml-runtime/src/abi.rs`) | nothing | — |

## Ideas and optimizations worth adopting

Ranked by expected impact on LotML.

1. **Make the parity suite a ratchet, not a boolean.**
   - **What.** Commit `harness/results/parity-floor.json` in pon's format. `--check-floor` fails
     when a floor program regresses or the count drops; `--update-floor` is an explicit, reviewed
     step; `--diff-floor` lists what regressed and progressed; filtered or sharded runs are
     refused for floor operations; `not compiled` messages are bucketed by `error[Exxxx]` code in
     the committed report.
   - **Why it fits.** LotML's parity is measured while the LLVM target is still closing gaps
     (ADR 0025 consequences). "Every program must pass" fails until the end, and a free-form
     report cannot stop a regression.
   - **ADR.** It touches specs/llvm-parity R5.1, and no ADR conflicts.
   - Gate it in `scc check` or CI the way `scripts/gate.sh` does: exit statuses captured
     directly, the summary line the only claim.
2. **Differential fuzzing between the two targets, with RC checks the CPython oracle cannot
   give.**
   - **Inputs.** Generate programs from LotML feature templates between `# FUZZ BEGIN/END`
     markers, plus mutants of corpus programs from `lotml dev mutate`
     (`compiler/crates/lotml/src/dev.rs:13`) that still type-check.
   - **Runs.** Run each on `lotml run` and on `lotml build` at `-O0` and `-O2`.
   - **Comparison.** Compare stdout, exit status, and the panic *kind*, the
     `panic: <kind>:` line, which is pon's stderr class.
   - **Minimization.** pon's greedy chunk deletion.
   - **The part pon cannot have.** Build with `-DLT_COUNT_CELLS` and assert
     `lotml: 0 cells live at exit` (`tests/common/mod.rs:98-112`), and set `LOTML_SANITIZE`.
     Every case then also checks reference counts, reuse and frees. Templates should push the
     edges pon's fast path got wrong (pitfall 2): values near `i64::MIN` and `i64::MAX`, `//` and
     `%` with negative operands, float `repr`, dict and set order.
   - **ADR.** ADR 0025 makes the Python target the reference, which this applies directly. No
     conflict.
3. **Keep backend-specific machinery out of the shared path, and test that it stays out.**
   - **The lesson.** pon's AoT executables carry JIT-only instrumentation: OSR polls, hotness
     bumps, safepoint polls (pitfall 1).
   - **The LotML analogue.** `lotml-ir` passes feed both a Python emitter and an LLVM emitter
     (ADR 0020). A pass added for one, such as counting or reuse, must stay out of what the other
     reads. ADR 0020 already orders it: Python reads before `mono`/`own`/`reuse`, LLVM after.
   - **A guard.** A test asserting that the Python backend's output contains no count operations,
     and that the LLVM output's runtime calls are exactly those `lotml.h` declares.
4. **Inline the runtime's hot fast paths into the generated code.**
   - **pon's evidence.** An out-of-line helper per operation is what dominates: its tier 1 exists
     to put the tag test and the arithmetic inline, leaving the helper for the cold side
     (`optimizing.rs:1645-1748`).
   - **LotML's version of the problem.** The LLVM backend calls `@lt_inc` out of line
     (`compiler/crates/lotml-llvm/src/emit.rs:1805-1813`). It exists only because `lotml.c:7`
     forces an external definition of the header's inline function. The `.ll` file and `lotml.c`
     are separate translation units with no LTO (`driver.rs:164`), so every count increment, and
     every drop's fast path, is a call.
   - **Options.**
     1. Emit `lt_inc`'s positive-count fast path directly in IR, calling the runtime only for a
        shared, negative count.
     2. Compile with `-flto` (on Windows, `-fuse-ld=lld`; `lld-link` ships with the LLVM
        installer) so that clang inlines across the two.
   - **Flag.** Option 2 adds a linker dependency on Windows. It fits ADR 0021 ("`clang` compiles
     it, with the C runtime"), but ADR 0021 names no linker, so record it in an ADR or in
     `docs/stack.md`. Measure both options on the benchmarks first.
5. **Self-containment check on Windows.** Run every executable the parity and leak tests build
   with a scrubbed environment: `PATH` without LLVM's `bin`, nothing from the developer shell. It
   catches a dependency on a sanitizer runtime or CRT DLL, or a path baked into the executable.
   Small, and it guards ADR 0022's promise of an executable with no interpreter to ship.
6. **Cache the compiled runtime object.**
   - **Today.** pon links a prebuilt archive it locates. LotML recompiles `lotml.c` on every
     build (`driver.rs:164`; with `lotml_text.c`, `lotml_list.c` and `lotml_dict.c` included, about
     4.9k lines).
   - **The change.** Compile it once per (clang version, level, `LT_COUNT_CELLS`, sanitizer,
     target, hash of the runtime source) into a cache, and pass the `.o`/`.obj`. This shortens
     every `lotml build` and test run.
   - **Flag.** ADR 0016 says the runtime is "compiled into each program as one translation
     unit". A cached object is still that one unit, and nothing is inlined across the boundary
     today. If idea 4 picks LTO, the cache must hold bitcode instead.
7. **Let the runtime own the entry sequence.**
   - **pon's shape.** The generated `main` is a single call, and init, argv, run, atexit, flush
     and exit status live in the runtime (`aot_entry.rs:64-132`).
   - **LotML today.** The LLVM backend writes the `Result`-returning `main` logic in IR text:
     error report, drop, `lt_exit` (`emit.rs:119-166`).
   - **The change.** Moving that into a `lt_run_main`-style runtime function makes it testable in
     C and one place to change. Low impact.
8. **Introspection for agents.** pon exposes a function's IR, CLIF and assembly from inside the
   language (`pon-jit/src/inspect.rs`). For an LLM-facing language, a `lotml` command that prints
   a symbol's IR and LLVM IR helps an agent explain a parity difference. Low impact.

**Not to adopt**, with the reasons:

- **Tiering, OSR, background compilation, inline caches, type feedback.** All of it exists to
  recover types that Python leaves unknown. LotML's checker knows them, `mono.rs` makes code
  monomorphic, and calls are direct. A JIT for `lotml run` would also contradict ADR 0021, which
  rejected Cranelift, and ADR 0025, under which `run` means CPython. The one static analogue of
  type feedback, profile-guided optimization with clang `-fprofile-use`, is not evidenced by pon
  and would be its own decision.
- **The uniform boxed `(argv, argc) -> obj` ABI and tagged integers.** ADR 0016 rejects boxing
  every value.
- **Interning ids baked into code.** LotML's strings are constants (count-0 static cells, ADR 0016).
- **A tracing GC.** See "Why not reference counting".

## Pitfalls seen

1. **JIT instrumentation leaks into AoT.**
   - Baseline lowering always passes `enable_osr = true` (`pon-codegen/src/baseline.rs:872-883`),
     so every back-edge in an AoT executable calls `pon_safepoint_poll` and `pon_osr_poll`
     (`baseline/control.rs:197-213`). `pon_osr_poll` does a current-function lookup
     (`abi.rs:5313`) for a JIT that is not there.
   - Every call bumps tier-up hotness (`abi.rs:5030`).
   - Sharing one codegen made the two paths agree, and made AoT pay for the JIT.
2. **An unsound fast path.** The typed tier lowers `+ - *` with wrapping `iadd`/`isub`/`imul`
   and Python's `//` and `%` with truncating `sdiv`/`srem` (`optimizing.rs:1773-1788`); the only
   guards are the small-int tag (`:1645-1659`) and a zero divisor (`:1706-1717`), and unannotated
   parameters are speculated `IntI64` (`infer.rs:82-88`). Overflow past 2^63 and negative floor
   division are therefore wrong whenever the typed path is taken. LotML does it right
   (`llvm.*.with.overflow`, `emit.rs:860`; floor fix-ups, `:966-967`); keep fuzz cases there
   (idea 2).
3. **A "perf substrate" that does not drive the optimizer.**
   - Tier 1 snapshots the feedback (`tierup.rs:650-663`), but `plan_function` never reads it
     (`:402-408`); the snapshot is only stored (`:572-575`).
   - Each hot function recompiles the *whole* IR module into a new `JITModule` (`:499-536`).
   - Threshold 16 calls or 10,000 back-edges (`abi.rs:87-92`).
4. **Process-local ids baked into artifacts.**
   - AoT must replay the build process's whole interner, in order, before runtime init.
   - The only check is a `debug_assert` that the interner did not grow after the snapshot
     (`pon-aot/src/lib.rs:180-201`).
   - Any id allocated later, at link or start-up time, misnames silently. LotML should keep
     emitting names as data, never as indices into a compile-time table.
5. **Two import paths.**
   - The JIT loads imports lazily from source (`pon/src/run.rs:201`).
   - AoT precomputes a closure (`reachable.rs:248-401`). "Best-effort" units that fail to compile
     are dropped with only a warning (`pon-aot/src/lib.rs:113-125`), and units past depth 32 are
     skipped (`reachable.rs:47-49`).
   - All 8 AoT parity failures are in this area. For LotML: module resolution must be one
     function both targets call, never re-derived per backend.
6. **Conservative GC whack-a-mole.**
   - Scrubbing argv arrays (`baseline.rs:2039-2081`), shadow slots (`:265-274`) and spill windows
     (`baseline/spill.rs:1-20`) were each added after a specific corpus failure (`weakref_basic`,
     `weakref_dicts`).
   - The README even contradicts the code on where precise maps exist: it says the typed tier
     has them (`README.md:60`), the code puts them in tier 0 only (`pon-jit/src/lib.rs:143-202`,
     `tierup.rs:530-533`).
7. **Static registries defeat dead-code elimination.**
   - `NATIVE_MODULES` holds a function pointer to every native module (`native/mod.rs:93`), so
     every executable links the 210k-line runtime, vendored OpenSSL and bundled SQLite
     (`Cargo.toml` `openssl = {features = ["vendored"]}`, `rusqlite = {features = ["bundled"]}`).
   - Hello world needs `-lncurses -lpanel -llzma` (`link.rs:28-29`).
   - If LotML's runtime grows a module table, keep each module in its own object, so that an
     unreferenced one stays out.
8. **Windows and cross-compilation were never designed in**: `cc` and `.a`, unguarded POSIX
   `libc` calls, x86-64 and AArch64 naked asm, a target triple combined with the host linker
   (Backend section).
9. **A racy global for source lines.** `pon_current_line` is one process-wide `AtomicU32`. The
   code admits it is wrong with several Python threads (`abi.rs:490-501`). LotML's `lt_at*`
   passed to the panic site (`lotml.h:57-71`) costs nothing on the success path and is
   thread-safe.
10. **Allocation per literal.** `str` literals allocate on every evaluation
    (`baseline/strings.rs:19-48`) and `int` literals are a helper call (`abi.rs:4563-4566`);
    LotML's count-0 static cells (ADR 0016) avoid both.
11. **Leaks by design.** `mem::forget(engine)` follows every dynamic `exec` in AoT
    (`pon-aot-dynamic/src/lib.rs:72-76`). `DynExecHandle` keeps a whole JIT engine per `eval`
    source (`pon-jit/src/lib.rs:45-50`).
12. **Documentation drift.**
    - The README's pass counts (`README.md:132`) and tier and stack-map statements (`:60`) lag
      the floors and the code.
    - The design notes (`plans/pon-pin-J0x-*.md`) are cited from code but are not in the
      repository.
    - The JIT keeps its own copy of the shared compile loop (`pon-jit/src/lib.rs:137-217`).
    - A second signature table, `PHASE_B_HELPERS`, sits beside `HELPERS`.

    Floors that are committed and checked stayed accurate; the prose did not.
