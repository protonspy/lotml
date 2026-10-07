# pycc — AOT compiler for strictly typed Python 3.14 to native binaries through LLVM, built by agents

github.com/rotnov/pycc · MIT (`LICENSE`; the vendored `tests/corpus/codecontests/` data is
CC BY 4.0, see its `NOTICE`) · last commit 2026-10-07 · Rust 1.97.1, edition 2024.
Rust 366k lines in all, 227k of them in test files. Non-test files (inline `#[cfg(test)]` modules
included, and 38–53% comment lines in the largest core files): `pycc_hir` 34.8k, `pycc_types` 33.5k,
`pycc_codegen` 23.8k, driver `src/` 22.7k, `pycc_rt` 9.4k, `pycc_mir` 8.3k, `pycc_diag` 2.5k,
`pycc_ast` 0.8k, `pycc_std` 0.8k, `pycc_parser` 0.16k. C 4.9k (the CPython extension shim and
embed launchers). Governance scripts about 60k lines of Python/Ruby. Markdown docs 7.1 MB, plus 255
decision records (`docs/decisions/D-*.md`). Maturity: pre-alpha, about 75 days of autonomous agent
work (first plan `docs/superpowers/plans/2026-07-24-v0-1-pr1-pr2.md`). Milestones v0.1–v0.3 are met
(`docs/ROADMAP.md:178`). The native surface is still narrow: `list[int]`, `dict[str, int]` and
`set[int]` only (T0034/T0036/T0038, `docs/DIAGNOSTICS.md:39-41`), and generics take one type
parameter.

Paths are relative to `research/prior-art/repos/pycc/`; `compiler/...` is LotML.

## Architecture

The target diagram in `docs/ARCHITECTURE.md` (lexer, THIR, `pycc_own`, SSA MIR, RC elision, salsa,
rayon, DWARF) is mostly unbuilt. The same file says so for MIR and `pycc_own`. What actually runs:

1. **Driver**: `src/main.rs` (`check`/`build`/`run`/`explain`/`lock`). `src/frontend.rs` and
   `src/modules.rs` load the project modules and resolve imports outside the compiler crates
   (D-222). `src/build_pipeline.rs:67` `try_build` sequences everything below.
2. **Parse**: `pycc_parser::parse_all` (`crates/pycc_parser/src/lib.rs:29`) calls
   `ruff_python_parser::parse_unchecked` (crates.io 0.0.6, `Cargo.lock:1020`) and maps every
   recovered error to `L0001`. `pycc_ast` (`crates/pycc_ast/src/lib.rs`) is a re-export facade over
   `ruff_python_ast`.
3. **HIR**: `pycc_hir::lower_module` (`crates/pycc_hir/src/module.rs:190`) per file, then
   `program::link` (`crates/pycc_hir/src/program.rs:95`) and `finalize` (`:374`) into one
   whole-program `HirModule`. Types: `Ty` (`crates/pycc_hir/src/lib.rs:101`), `HirExpr` (`:400`),
   `HirStmt` (`:984`), `HirItem` (`:1382`).
4. **Types**: `pycc_types::check_and_resolve_all` (`crates/pycc_types/src/module.rs:145`) returns
   HIR again, with signatures solved and generics expanded. There is no THIR.
5. **MIR**: `pycc_mir::build` (`crates/pycc_mir/src/lib.rs:1844`) gives `MirModule { items,
   class_defs }`: `MirExpr` (`:86`), `MirStmt` (`:1438`), `MirItem` (`:1796`).
6. **Codegen**: `pycc_codegen::compile_to_object_with_options`
   (`crates/pycc_codegen/src/lib.rs:5767`), through inkwell 0.9 / LLVM 22.1 (`llvm22-1` feature),
   writes one `.o` file.
7. **Link**: `cc`, or the bundled `clang`, runs on the object with `-lpycc_rt`
   (`src/build_pipeline.rs:184-205`). `pycc_rt` is a Rust `staticlib`.

`run` and `build` share every stage: `run` builds into a scratch directory and executes the
binary (`src/main.rs:365`). There is no interpreter or JIT. The semantic oracle is an external,
pinned CPython 3.14.7 running the same `.py` file. LotML's oracle is instead its own Python target
(adr:0025).

## Frontend

- **Not hand-written.** It is ruff's recovering parser. D-003 promises an own parser "before v0.6";
  D-017 deferred the separate lexer crate. Every syntax error is reported in ruff's discovery order,
  which is deliberately not sorted (`crates/pycc_parser/src/lib.rs:6-20`). The only extra pass is
  string-annotation unquoting (`crates/pycc_ast/src/string_annotation.rs`). "Zero-copy" here means
  ruff's AST carries byte ranges. pycc adds nothing to that.
- **No incremental or parallel frontend.** No `salsa` or `rayon` appears in any manifest, and there
  is no LSP. The speed gate is `pycc check` on a fixed 1000-line fixture under 50 ms, taken as the
  minimum of 5 samples (D-084, D-174). A separate paired base-vs-head Criterion run blocks
  regressions above 2% (D-051, D-062).
- **Error recovery past parsing** is per item. A failed HIR item poisons its bindings, and
  diagnostics that cascade from them are suppressed (D-219, `crates/pycc_hir/src/module.rs:40`
  `mod poison`). The checker collects one diagnostic per function (D-220).
- **Spans are lost after HIR.** `HirExpr` has no span field, and `pycc_types` builds
  `Span::new(0, 0)` at 207 sites. Type errors therefore point at `1:1` or at the start of the
  function: `tests/diagnostics/t0021_annotated_broken_value_still_reports_the_real_defect.expected.txt`
  reports a body error at `1:1` on `def f() -> int:`.
- **Compared with LotML:** `lotml-syntax` is hand-written, tolerant and indentation-aware, behind
  salsa (`lotml-db`), and its IR keeps `span`/`at` on every statement
  (`compiler/crates/lotml-ir/src/ir.rs:295-301`). pycc has nothing to reuse here (ruff is studied
  separately).

## Semantics and types

- **Strictness.** Public signatures must be annotated (T0001). `Any` is forbidden outside the
  `--ext` boundary (T0002). `eval`, `exec`, monkey-patching, three-argument `type()` and `import *`
  are refused with fixed codes (`docs/PYTHON_STANDARDS.md:458-470`). Any syntactically valid
  construct outside the implemented subset gets a spanned `C0001` capability diagnostic, never a
  panic (`docs/ARCHITECTURE.md`, "Syntactically valid constructs outside that implemented HIR
  subset return a spanned C0001").
- **Inference.** Private helpers (leading `_`) may omit annotations. A union-find constraint solver
  (`crates/pycc_types/src/constraints.rs`, D-038/D-045) gives them monomorphic signatures, which an
  annotation-driven checker (`crates/pycc_types/src/lib.rs`) then re-validates. These two phases
  re-derive the same admission conditions, and that duplication caused a 9-round review loop (see
  Pitfalls).
- **Pre-passes**, all HIR to HIR:
  - `resolve_empty_containers` (`crates/pycc_types/src/empty_container.rs:204`, D-245) fixes the
    element type of `[]`/`{}` before checking.
  - Attribute-slot resolution.
  - `inherited_copies` (`crates/pycc_types/src/inherited_copies/plan.rs:208`, D-254).
- **Narrowing.** A side-table overlay narrows `Optional` after `is None` / `is not None`
  (`crates/pycc_types/src/narrow.rs:119-257`, D-205).
- **Generics.** `monomorphize` (`crates/pycc_types/src/monomorphize.rs:3108`) rewrites HIR to HIR.
  - The body is checked once with `Ty::Param` as an opaque type.
  - Each call site is instantiated by `instantiate_generic_call` (`:395`) and mangled to
    `0gen_{fn}__{T}_{ty}` (`:94`). The leading digit cannot collide with a user name.
  - `substitute_ty` is deliberately not recursive (`:111`), because T0042 refuses more than one
    type parameter and any container position such as `list[T]`
    (`crates/pycc_types/src/lib.rs:3483`).
  - Protocol-typed parameters are monomorphized the same way (D-006).
  - LotML's `mono.rs` is well ahead: several type arguments, nested types, lambdas per instance,
    `dyn` vtables, and limits on instances and type size
    (`compiler/crates/lotml-ir/src/mono.rs:18-80`).
- **Classes.** Classes are heap instances with flat slot arrays behind FFI accessors (D-154).
  Dispatch is static only: no vtables and no runtime type tags (D-006, D-160).
  - Static dispatch of inherited bodies silently miscompiled `self.m()` overrides, `super()` and
    class attributes. The fix (D-254) compiles an inherited body again per receiver class when a
    fixpoint "copy-needed" rule says its behaviour differs, and refuses programs where `self`
    escapes as the base type.
  - Multiple inheritance whose base layouts are not prefixes of the derived layout is refused
    (D-234).
  - A debug-only verifier re-derives every method callee (`crates/pycc_mir/src/verify_receiver.rs:47`).
  - LotML has no classes or inheritance (`reference/lotml.md:281`); traits with `dyn` are its open
    world.
- **`match` exhaustiveness** is per arm, not a decision tree (`crates/pycc_types/src/lib.rs:1878`,
  D-169).
  - Exhaustive means an unguarded irrefutable arm, `True`+`False`, or every enum member; anything
    else is T0030.
  - LotML's `exhaustive` (`compiler/crates/lotml-check/src/body.rs:586`) has the same power over
    sums, `Result`, `bool` and `Optional`, and also offers a fix-it with the missing arms. Nothing
    to take.
- **Dynamic features.** CPython objects are an opaque `Ty::Object` reached only through `--ext` or
  embedded imports (D-244, D-258). Native builds refuse the rest.

## IR and passes

- **HIR desugaring.** Comprehensions, decorators, properties (an attribute rewrite, D-158), enums
  (singleton instances, D-163) and string annotations. `crates/pycc_hir/src/`.
- **HIR link/finalize.** Whole-program linking with poison cascade suppression
  (`crates/pycc_hir/src/program.rs:95,374`).
- **Empty-container pre-pass** (`crates/pycc_types/src/empty_container.rs:204`).
- **Inherited-body copies** (`crates/pycc_types/src/inherited_copies/`).
- **Private-helper solver**: union-find over `TypeTerm` (`crates/pycc_types/src/constraints.rs`).
- **Checker + narrowing** (`crates/pycc_types/src/lib.rs`, `narrow.rs`).
- **Monomorphization**, HIR to HIR (`crates/pycc_types/src/monomorphize.rs:3108`).
- **Enum loop unrolling**: one item per member (`crates/pycc_types/src/enum_lower.rs:154`).
- **MIR build**: a "typed structural mirror of HIR", explicitly not SSA (D-057).
  - Trees of recursive `MirExpr` over string-named variables.
  - Type-specialised nodes: `ForList`/`ForDict`/`ForSet`/`ForRange`, `ListAppend`, `DictSet`,
    `ListCompAssign`/`DictCompAssign`/`SetCompAssign`, `Try`/`TryStar`
    (`crates/pycc_mir/src/lib.rs:1438-1763`).
  - `match` lowers to an `if` chain with a `__match_subj_N` temporary (`:58`) and `Unreachable`
    for exhaustive fallthrough (`:1454`).
- **Foreign-import splice**, which keeps a CPython `import` at its source position
  (`crates/pycc_mir/src/lib.rs:1967`).
- **Receiver verifier**, debug builds only (`crates/pycc_mir/src/lib.rs:1937`).
- **No MIR optimisation, ownership or RC pass exists.** Reference counting is emitted ad hoc inside
  codegen per type (`crates/pycc_codegen/src/bigint_rc.rs`, 2374 lines; `str_rc.rs`), following
  D-180/D-181/D-182/D-208/D-212.
  - Exception edges keep a `pending_int_releases` stack.
  - `--release` runs LLVM `default<O3>` on the module (`crates/pycc_codegen/src/lib.rs:129,6886`).
- **Compared with LotML's `lotml-ir`.** `ir.rs` is structured too, but in A-normal form:
  `Operand::Local | Const` over numbered `Local`s with types (`ir.rs:265-392`), plus a
  backend-neutral `Builtin` operation enum (`ir.rs:13`) rather than one node per container type.
  - `own.rs` inserts `Inc`/`Dec` from liveness (`ir.rs:369-377`).
  - `reuse.rs` turns a `Dec` followed by a constructor into `DropReuse`.
  - `hoist.rs` lifts uniqueness checks out of loops.
  - `verify.rs` runs after each pass (`compiler/crates/lotml-ir/src/lib.rs:17-26`).
  - LotML is architecturally ahead on every point. pycc is evidence of what skipping the
    ownership pass costs (see Pitfalls).

## Backend and toolchain

- **LLVM through inkwell 0.9** (`crates/pycc_codegen/Cargo.toml`, D-015), pinned to one LLVM
  release (22.1.1). Building the compiler needs `LLVM_SYS_221_PREFIX`, and on Windows also a
  vcpkg `libxml2`, because the official LLVM release links it (D-027).
  - Windows bug 1: `LLVMDisposeMessage` crashes. Every `LLVMString` is leaked with
    `ManuallyDrop`/`mem::forget` (`crates/pycc_codegen/src/lib.rs:6914`), and `module.verify()` is
    skipped on Windows (`:6936`, D-029).
  - Windows bug 2: `Target::initialize_all` races with `create_target_machine`, so it is guarded by
    a `OnceLock` (`crates/pycc_codegen/src/target_machine.rs:1-70`).
- **Codegen shape.**
  - One `alloca` per local and parameter, left to mem2reg (D-057).
  - Values are `Scalar::{Int, Bool, Float, Str, List, Dict, Set, Tuple, Optional, Instance}`
    (`crates/pycc_codegen/src/lib.rs:148`).
  - Tuples are SSA structs held by value (D-115). `Optional` is `{inner, i8}`.
  - The target machine uses the `"generic"` CPU and `RelocMode::PIC`. PIC is needed because
    Ubuntu's gcc links PIE (`target_machine.rs:84-110`, D-073).
  - There is no debug info: no DIBuilder anywhere, although `docs/ARCHITECTURE.md` promises
    DWARF/PDB. LotML already writes `DISubprogram`/`!dbg` (`compiler/crates/lotml-llvm/src/emit.rs:437-545`).
- **Calling convention.** Plain `extern "C"` into `pycc_rt` for every runtime operation, integer
  arithmetic included (`crates/pycc_codegen/src/lib.rs:2411-2431`). Exceptions are a thread-local
  flag checked after the operations that may raise; LLVM never sees unwinding (D-173).
- **Finding a linker.** The linker path is fixed at compile time
  (`src/build_pipeline.rs:469-517`):
  - Windows: `env!("LLVM_SYS_221_PREFIX")/bin/clang.exe`, with `-target x86_64-pc-windows-msvc`
    forced (`:529-532`, D-028). A bare `clang.exe` sometimes picked a MinGW `ld` from `PATH`.
  - Linux with `--target`: the same bundled clang, because GCC rejects `-target` (D-031).
  - Otherwise: the system `cc`.
  - Windows also needs rustc's system-library list (`ws2_32` … `legacy_stdio_definitions`,
    `:548-564`), because the runtime is a Rust staticlib.
  - Linux needs `-lm` (`:583`).
  - CI needs a Visual Studio developer prompt (`ilammy/msvc-dev-cmd`) so that `lld-link` finds
    `libcmt.lib` (D-028 point 5).
- **Locating the runtime.** `libpycc_rt.a` / `pycc_rt.lib` is looked up in the source checkout's
  Cargo target directory (`crates/pycc_artifact_layout/src/lib.rs:188`). A nested
  `cargo build -p pycc_rt` inside `crates/pycc_codegen/build.rs` installs it there (D-184). A
  `pycc` binary is therefore not relocatable, and `docs/DISTRIBUTION.md` records that there is no
  distribution yet.
- **Cross-compilation.** `--target <triple>` passes the triple to LLVM. The runtime has to be built
  by hand with `rustup target add` + `cargo build --target`. Only same-OS cross-arch is proven
  (macOS arm64↔x86_64, D-026), because cross-OS needs a sysroot.
- **Shared libraries.** `pycc build --ext` makes an abi3 CPython extension module.
  - It compiles a fixed 4.6k-line C shim (`src/ext/pycc_ext_module.c`, `Py_LIMITED_API`
    0x030D0000) plus a generated `.inc`, beside the object.
  - The interpreter is probed with `python -I -c` (`src/ext_build.rs:234`).
  - The link arguments are a pure function of the target triple, not the host
    (`src/ext_build.rs:288-402`): macOS `-bundle -undefined dynamic_lookup`; Linux `-shared`
    `-Wl,-Bsymbolic`, so two modules loaded `RTLD_GLOBAL` do not interpose; Windows `-shared`
    `-lpython3`.
  - "Embedded" executables bundle a pinned CPython closure. On Windows that is a stub `.exe` plus
    a program DLL (D-248, D-253).
- **Compared with LotML** (adr:0021/0025, `compiler/crates/lotml-llvm/src/driver.rs`). LotML
  writes textual IR, looks up `clang` at run time (`LOTML_CLANG`, then `PATH`, then
  `%ProgramFiles%\LLVM`, `driver.rs:36-90`), and compiles the C runtime from source with the same
  `clang` on every build (`driver.rs:138-175`). That design avoids D-015/D-027/D-029, the
  baked-in paths, the runtime-location machinery (D-183/D-184) and the MSVC-staticlib ABI mismatch
  behind D-028. The runtime and the program always come from one compiler.

## Runtime

`crates/pycc_rt` is pure Rust with no dependencies: `crate-type = ["staticlib", "rlib"]`.

- **int.** One word, low-bit tagged (D-061/D-141):
  - odd words are 63-bit smallints;
  - words `2`/`6` are `False`/`True` markers;
  - aligned non-zero words are heap `BigIntObj` pointers with `u32` limbs (`docs/RUNTIME.md:23`).
  - Every operation is an exported `extern "C"` call. Example: `pycc_rt_int_add`
    (`crates/pycc_rt/src/lib.rs:213`) wraps `int_add` (`:167`), which promotes to a bigint through
    `i128`.
  - Decimal printing of a bigint uses repeated divmod-by-10 (`:102-130`), which is O(n²).
  - LotML's `int` is i64 with checked overflow (`compiler/crates/lotml-runtime/c/lotml.h:174-215`),
    emitted inline with LLVM's checked intrinsics.
- **str.** UTF-8 (D-007).
  - `PyStrObj { rc: Cell<u32>, payload: Inline([u8; 22], u8) | Heap(Box<[u8]>) }`
    (`lib.rs:886-906`, D-059).
  - A literal allocates a new object every time it is evaluated (`pycc_rt_str_from_literal`,
    `:994`). LotML uses count-zero static strings (adr:0016).
  - Unbound temporaries are never decref'd (`docs/RUNTIME.md:25`, #1054).
- **Containers.** Each element combination has its own runtime type:
  - `list[int]`: `Cell<Vec<i64>>` (`lib.rs:1401`).
  - `dict[str, int]`: a dense array with **linear-scan** lookup (`lib.rs:2160`,
    `docs/RUNTIME.md:27`).
  - `set[int]`: linear-scan dedup (`crates/pycc_rt/src/int_set.rs`), with an iteration order that
    is not CPython's.
  - The incref/decref functions exist (`lib.rs:1976,1995,2313,2329`), but nothing outside
    `pycc_rt` calls a container decref, so lists, dicts and sets leak for the life of the process
    (D-107, D-124).
  - Instances (`crates/pycc_rt/src/instance.rs:81`, `slots: Cell<Vec<Option<i64>>>`) are never
    freed.
  - No cycle collector exists, although D-004 plans Bacon-Rajan.
  - LotML's runtime is ahead on every one of these: SipHash-1-3, the CPython set table, type
    descriptors and Perceus reuse.
- **Exceptions** (D-173, superseding D-005's native unwinding):
  - The thread-local `ExceptionState { active: i8, value: *mut PyExceptionObj }`
    (`crates/pycc_rt/src/exception.rs:102-115`) holds the pending exception.
  - A raise allocates the object and sets the flag. Codegen checks the flag only after nodes that
    can set it (`expression_can_set_exception`, `crates/pycc_codegen/src/exception.rs:39`;
    `guard_statement_effects`, `:330`), so plain arithmetic stays branch-free.
  - Each function has an exception-exit block that returns a neutral value. Classes have integer
    tags. A handler tests a sorted set of tags (the class plus every subclass, computed at compile
    time; `pycc_rt_exception_type_matches`, `exception.rs:447`).
  - setjmp/longjmp was rejected because it would skip `finally`, and unwinding because of the
    per-platform cost. That is errors-as-values implemented by hand. LotML gets the same behaviour
    from the type system (`T ! E`, adr:0002). Nothing to copy.
- **Runtime panics** abort at the `extern "C"` boundary (Rust ≥1.71 abort-on-unwind,
  `lib.rs:132-215`). Every symbol is split into a private body plus a `no_mangle` wrapper so that
  `#[should_panic]` tests still work.
- **Leak probes.** `pycc_rt_str_live_objects` and `pycc_rt_buffer_live_views` (`lib.rs:926-943`)
  are relaxed atomic counters read around a call sequence. They correspond to LotML's
  `LT_COUNT_CELLS` (`driver.rs`, `counting`).
- **CPython interop.** `ext_bridge.rs` holds the pure encode/decode half. The `PyObject*` moves live
  in the C shim, so a native executable never links libpython.

## Testing and conformance

`docs/TESTING.md:7-18` now lists 10 layers. The original seven are 1–7; 8–10 cover the ext,
embedding and interop policy.

1. **Unit tests**: `#[cfg(test)]` per crate.
2. **Differential conformance**: 79 tests named `*_matches_cpython_3_14_7_byte_for_byte` in
   `tests/conformance.rs` and the cohorts in `tests/conformance/*.rs`. The fixtures are
   `tests/fixtures/*.py`.
   - Each test builds `--debug` and `--release`, runs the binary, runs the oracle, and compares
     stdout bytes (`tests/conformance.rs:162-205`).
   - The oracle must be exactly `Python 3.14.7` (`:111-123`).
   - CPython's Windows CRLF translation is stripped (`:136`, D-082).
   - Text may be exempted from the byte comparison only through a listed exception table, which
     is empty today (`docs/TESTING.md:222-236`).
3. **Diagnostics**: 201 `tests/diagnostics/<code>_<slug>.py` sources with exact `.expected.txt`
   human and JSON snapshots. The snapshot harness is hand-rolled, not insta (D-036).
4. **Differential fuzzing**: planned, absent.
5. **Runtime property tests**: claimed as "`pycc_rt` proptest", but no proptest or quickcheck
   appears anywhere in the tree, and the "planned" note covers only layers 4 and 6.
6. **Open-source corpus**: planned, absent.
7. **Benchmarks.**
   - `benches/check_bench.rs` (Criterion, frontend).
   - The nbody ratio gate (`tests/nbody_bench.rs:676`). Its floor is 20× CPython, lowered to 18×,
     15× and 12× for Linux aarch64, Windows and macOS aarch64 (D-095, D-096, D-101).
   - The frontend floor and paired-regression gates (D-051, D-062, D-174).

The conformance matrix sits on top of those layers:

- **`docs/PYTHON_STANDARDS.md`** maps every PEP up to 3.14 to a fixture path and a status:
  ☐ planned, ◐ subset, ✅ whole PEP.
- **`tests/conformance_matrix_guard.rs`** checks that every ◐/✅ row cites a registered fixture,
  and that every `pep_*.py` fixture is registered.
- **`tests/fixtures/conformance-breadth-manifest.json`** records, per evidence-backed row, what the
  fixtures prove and what they do not (`not_proven`). Each gap is classified `core` or
  `out-of-scope`.
- **`scripts/check_conformance_breadth.py`** derives each row's status mechanically from that
  manifest: any `core` gap forces ◐, and ✅ requires none (D-176/D-177). It also checks the totals
  quoted in `docs/ROADMAP.md`.
- **`tests/conformance_oracle_guard.rs`** parses the harness's Rust text to prove that each
  differential test really compares pycc against the oracle, not `x` against `x`. It
  mutation-tests itself.

The informational corpus and the coverage gate:

- **Corpus** (`tests/corpus/codecontests/`): 200 steering and 100 holdout CodeContests Python
  solutions, each with public and private stdin/stdout tests.
  - `scripts/check_corpus_compile_rate.py` reports `compiled N/M`, `matched K/N` and the median
    speedup. Speedup counts only problems where CPython takes at least 200 ms, timed on the
    largest case, best of 3.
  - The holdout set is "never read when choosing what to implement" (`docs/TESTING.md:323-418`).
- **Coverage**: 100% line coverage of the Rust lines a PR adds or changes
  (`scripts/check_diff_coverage.py`, LCOV joined with `git diff -U0`, D-242). It started as 100%
  line and region coverage of the whole workspace (D-014).

## Reusable for LotML

| Item | Path in pycc | What it gives | LotML crate | License verdict | Effort |
|---|---|---|---|---|---|
| Binary dependency readers | `src/embed/pe.rs` (325 lines: `parse_pe` `:174`, `parse_forwarders` `:200`), `src/embed/elf.rs` (220, `parse_elf` `:89`), `src/embed/macho.rs` (`parse_otool_l` `:52`) | Pure, bounds-checked readers for PE imports and delay-imports and ELF `DT_NEEDED`; std only, fuzz-safe | `lotml-llvm` tests: assert a native exe or `--shared` library imports no `python3*.dll`/`libpython` (adr:0025) and only the expected DLLs; add export-directory names to `pe.rs` to check the export set (adr:0024) | adapt with attribution (MIT) | S (imports) / M (+exports) |
| Link arguments as a pure function of the target triple | `src/ext_build.rs:288-402` (`ExtLinkPlatform::from_target_triple`, `ext_link_args`) | Per-OS shared-object link arguments, unit-testable on one host; `-Wl,-Bsymbolic` against ELF interposition; `Path::join`, never `format!`, for cross-host paths | `lotml-llvm/src/driver.rs:138-175`, which uses `cfg!(windows)` today | adapt (MIT) | S |
| Changed-line coverage | `scripts/check_diff_coverage.py` (stdlib Python; `parse_lcov` `:94`, `parse_added_lines` `:129`, `evaluate` `:191`) | Coverage of the lines a branch touches, from the same `cargo llvm-cov --lcov` export | `harness/scripts/test_gate.py`, as a report beside the total floor | copy with attribution (MIT) | S |
| Breadth manifest and checker | `scripts/check_conformance_breadth.py`, `tests/fixtures/conformance-breadth-manifest.json` | "Proven / not proven (core vs out-of-scope)" per feature row, with the status derived and never hand-set | parity status of `reference/lotml.md` sections × {python, llvm}, under specs/llvm-parity | idea only (the script is bound to their Markdown) | M |
| Corpus metric with holdout | `scripts/check_corpus_compile_rate.py`, `scripts/select_codecontests_corpus.py`, `tests/corpus/codecontests/` | Compile rate, match rate and speedup with a 200 ms floor and an `INCOMPLETE` budget, over a steering/holdout split; 300 problems with hidden tests | `harness/` agent evaluation, RL tasks (wiki `verifiable-rewards`, `rl-environment`) | scripts: adapt (MIT). **Data: CC BY 4.0**, attribution required, and problem statements are not vendored | M |
| Exact oracle pin | `tests/conformance.rs:111-123` | The test fails unless the oracle interpreter is exactly the pinned patch version | parity CI for `lotml run`'s CPython (float repr, hash and set order are CPython copies, adr:0016) | idea only | S |
| Hashes for int, tuple and identity | `crates/pycc_rt/src/hash.rs` | CPython `long_hash`, `tuplehash`, `_Py_HashPointer` | none needed: LotML has `lt_hash_i64`/`lt_hash_tuple` (`compiler/crates/lotml-runtime/c/lotml_list.c:13,55`) | nothing | — |
| Parser, HIR, MIR, codegen, runtime | `crates/*` | — | none: LotML's IR, mono, ownership and runtime are ahead; its syntax is not Python | nothing | — |

## Ideas and optimizations worth adopting

1. **Keep adr:0021's textual IR plus a `clang` found at run time, and treat pycc as the evidence for
   it.** pycc's inkwell path paid for:
   - a pinned LLVM development install and vcpkg `libxml2` on Windows (D-015, D-027);
   - Windows crashes that forced skipping `module.verify()` and leaking every `LLVMString` (D-029);
   - linker paths baked in with `env!` (`src/build_pipeline.rs:481`);
   - a runtime that `pycc build` can only find inside a Cargo target directory
     (`crates/pycc_artifact_layout/src/lib.rs:188`), so there is no distributable binary;
   - a Windows ABI mismatch between the MSVC Rust staticlib and a MinGW link (D-028).

   LotML's runtime-from-source build makes the whole class impossible. Binds adr:0021/0025, with no
   conflict. One residue to check: pycc's D-073. A `.o` emitted with `RelocMode::Default` failed to
   link as a PIE on Ubuntu. LotML gives `clang` the `.ll` itself, so the driver's default PIE
   applies. Keep it that way; never emit objects separately without PIC.
2. **A mechanical parity matrix for adr:0025's parity suite.** Map each `reference/lotml.md`
   section to its fixtures and a status on the Python and LLVM targets. Record the breadth: what
   the fixtures prove, and the `core` and `out-of-scope` gaps. Derive the status from that, so a
   narrow test cannot claim a whole feature (pycc's D-177 lesson). Make the suite **data-driven**
   (a fixture directory, one harness) rather than one hand-written `#[test]` per program as
   `compiler/crates/lotml-llvm/tests/milestone.rs` does today. A data-driven harness makes pycc's
   matrix guard and oracle guard unnecessary, along with their hand-written Rust-text parser. Add
   an explicit, empty-by-default exemption table for any text not compared byte for byte. No ADR
   conflict; it lives in specs/llvm-parity.
3. **Link arguments keyed by target triple, plus `-Wl,-Bsymbolic` for `--shared` on ELF**
   (adr:0024). LotML's `-fvisibility=hidden` (`driver.rs:158`) hides the runtime. `-Bsymbolic` also
   binds the exported functions' internal references to themselves when two LotML libraries are
   loaded `RTLD_GLOBAL`. Making the argv a pure function of the triple is also the cheapest way to
   cross-arch builds: `clang --target=<triple>` compiles both the `.ll` and the runtime, which a
   Rust staticlib cannot do. It lives in `driver.rs`, with no conflict. On Windows, consider
   passing the triple explicitly as well, so that a MinGW-default `clang` (MSYS2) is a choice and
   not an accident (D-028). LotML is immune to D-028's ABI mismatch, but not to an unexpected CRT.
4. **Artifact-shape tests.** Use pycc's PE/ELF readers to assert that a native exe never depends
   on libpython (adr:0025) and that a `--shared` library exports exactly the C ABI set (adr:0024).
   These are cheap, deterministic, and need no Python.
5. **Corpus metric with a holdout set for the agent harness.** Report compile rate, match rate and
   speedup over a steering set, and keep a holdout set nobody reads while choosing what to build.
   This stops the language and compiler from being tuned to the benchmark, which matters for
   LotML's HumanEval and RL evaluation (adr:0015, wiki `reward-hacking`). The CodeContests problems
   need stdin input, which `reference/lotml.md` does not offer: adding it is a language change
   (prelude), so it is flagged rather than assumed.
6. **A `lotml check` throughput floor taken as the minimum of N samples** (D-174: runner noise only
   adds time, so the minimum estimates the true cost). It protects adr:0006's claim that the speed
   of the agent loop is the frontend's. LotML has no frontend timing gate today. No conflict.
7. **Coverage of the C runtime itself.** pycc measures its Rust runtime through the integration
   binaries that link it (`docs/TESTING.md`, "A runtime function an integration test links is
   measured from that binary"). LotML's `lotml.c` is compiled by `clang` at test time, so
   `cargo llvm-cov` never sees it. A `LOTML_COVERAGE` switch, built like `LOTML_SANITIZE`
   (`-fprofile-instr-generate -fcoverage-mapping`), over the parity programs would give
   runtime coverage. No conflict.
8. **Changed-line coverage as a report** beside the gate's total floor (`check_diff_coverage.py`).
   A report, not a gate: pycc's own retrospective has a 100%-covered diff that shipped a
   host-aborting null dereference (`docs/AGENT_RETROSPECTIVE.md`, 2026-09-21).
9. **A CPython extension module as an output of `build --shared`** (pycc's product pivot D-244: an
   abi3 module, "≥5× CPython" kill criterion). It would let Python call native LotML code.
   **Conflict flag:** adr:0025 chose "`run` on CPython, `build` to native code, nothing else" and
   rejected embedding. An extension is loaded *by* CPython, so it does not contradict the letter of
   0025, but it reopens scope that 0025 closed and touches 0012/0024. It needs a new ADR. L.
10. **Process ideas worth keeping** (from `.claude/skills/issue-to-plan/SKILL.md` and
    `docs/AGENT_RETROSPECTIVE.md`):
    - "The issue text is dated evidence, not a specification": verify every claim of a plan
      against the tree before relying on it.
    - When a reviewer's counter-example names one condition of a predicate that mirrors another
      phase's predicate, enumerate both full condition sets instead of patching the named one. A
      second review round on the same seam is the signal.
    - A local gate run differently from CI (`--include-ignored`) is a different gate.
    - One writer per worktree: a subagent's report does not mean it has terminated.

    LotML's scc rules already cover planning, delivery and notes. These are additions for
    `docs/notes.md` or `.claude/rules`, not new machinery.

## Pitfalls seen

- **Ownership deferred, so reference counting grew by accretion.** `pycc_own` was "confirmed out
  of scope" (D-060) and never built. RC grew per type inside codegen instead:
  - `bigint_rc.rs` reached 2374 lines, plus `str_rc.rs`, five decision records
    (D-180/181/182/208/212) and a `pending_int_releases` stack for exception edges;
  - lists, dicts, sets and instances still leak, since no container decref is ever called;
  - unbound `str` temporaries leak (#1054).

  LotML's choice to insert counts as an IR pass from liveness (`own.rs`, adr:0016) is the
  difference. Keep RC out of the emitters.
- **A runtime call for every operation caps speed.** Every `int` operation is an opaque
  `extern "C"` call on tagged words (`crates/pycc_codegen/src/lib.rs:2411-2431`), `float ** float`
  is a runtime call, and string literals allocate on each evaluation. nbody reaches about 18×
  CPython. D-095 suspects the `pycc_rt_float_pow` call the module cannot inline, and three
  per-platform relaxations of the gate followed (`tests/nbody_bench.rs:676`, D-095/D-096/D-101).
  pycc's runtime is a Rust staticlib linked without LTO, so no cross-boundary inlining is possible.
  LotML keeps arithmetic inline with checked intrinsics; keep hot helpers out of opaque calls.
- **A structural-mirror MIR with type-specialised nodes.**
  - `ForList`/`ForDict`/`ForSet`, `*CompAssign`, `ListAppend`/`DictSet`, string-named variables,
    and one runtime struct per element combination.
  - So every element type is a new gate: `list[int]` only (T0034), `dict[str, int]` only (T0036),
    `set[int]` only (T0038), and generics restricted to one type parameter with no `list[T]`
    (T0042).
  - Generality postponed at the IR level becomes a diagnostic per combination. LotML's `Builtin`
    enum over typed locals plus type descriptors avoids this.
- **Static dispatch with inheritance and no vtables miscompiled silently.** Template-method
  overrides, `super()` in diamonds and class attributes all went wrong. The fix (D-254) is a
  fixpoint copy analysis and a refusal of escapes. D-234 refuses some multiple inheritance. LotML's
  "no classes, traits + `dyn`" (`reference/lotml.md:281`) avoids this; keep it.
- **Spans dropped at HIR.** 207 `Span::new(0, 0)` sites in `pycc_types`, so type errors point at
  `1:1` or at the function header. For a language whose users are agents (LotML), the location is
  the product. Never lower to a span-less form before checking.
- **Two inference engines for one language.** The private-helper solver and the annotation checker
  each re-derive admission rules, and `merge_solver_first` lets the incomplete one win. The result
  was wrong diagnostics found one per review round, nine rounds on one PR
  (`docs/AGENT_RETROSPECTIVE.md`, 2026-09-21). LotML has one checker; keep a single source of
  typing truth.
- **Documentation outran the code.**
  - ARCHITECTURE describes a lexer crate, `pycc_own`, THIR, SSA MIR, salsa, rayon and DWARF/PDB;
    none exists.
  - TESTING layer 5 claims proptest, which is absent.
  - D-169 justifies T0030 as matching "CPython's own `MatchError` runtime behavior", but CPython has
    no `MatchError`: a `match` that matches no arm does nothing. pycc therefore refuses valid
    Python while believing it matches CPython.

  Claims need checks. LotML's scc validation and the "describe what is" convention guard against
  this.
- **Process apparatus ate the product.** In about 75 days:
  - 255 decision records, 7.1 MB of Markdown, a 388 KB retrospective, ~60k lines of governance
    scripts with their own tests (`scripts/test_check_roadmap_evidence.rb` is 218 KB);
  - 206 test files named by issue number (`tests/issue_*.rs`) instead of by feature;
  - 38–53% comment lines in the largest core files, much of it change history ("PR-11 Task 4/D-123");
  - roadmap paragraphs thousands of words long.

  D-242 ("product mode") had to roll back the whole-workspace 100% coverage, the byte-exact CI
  audits and the per-PR journals after they "consume[d] most of a run's wall-clock and tokens".
  LotML's caveman register, one-line notes (`docs/notes.md`), "no history in comments", ADR bar
  and feature-named tests are the right side of this. Hold them.
- **Windows with LLVM-C.** `LLVMDisposeMessage` crashes, `module.verify()` is skipped on Windows,
  `Target::initialize_all` races (D-029, `target_machine.rs`), and a bare `clang.exe` picked
  MinGW. Every one is inkwell- or staticlib-specific, and is why adr:0021's route matters.
- **Cross-compilation is mostly a sysroot problem, not a codegen one** (D-026). LLVM accepts any
  triple; linking needs that OS's CRT and headers. If LotML adds `--triple`, scope it to
  same-OS cross-arch first, as pycc did.
- **Gate noise.** Single-sample timing gates flapped until pycc moved to the minimum of 5 (D-132
  was superseded by D-174). The paired base-vs-head gate needed half a dozen decision records of workflow trust
  machinery (D-048…D-062). Measure perf as the minimum of N and report it before blocking on it.
