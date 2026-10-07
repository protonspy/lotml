# spy — a static Python variant: one AST evaluator that both interprets and partially evaluates ("redshift") into C

github.com/spylang/spy · MIT (`LICENSE`, © 2023 spylang; vendored Ryu is Apache-2.0 OR BSL-1.0) ·
last commit 2026-10-06 (shallow clone) · Python 30.0k lines without tests (`spy/vm` 14.6k,
`spy/backend` 3.3k, `spy/analyze` 2.2k, `spy/cli` 1.4k, `spy/build` 0.8k, `spy/llwasm` 0.6k,
top-level modules 7.1k: `parser.py` 1.2k, `ast.py` 1.2k, `astcompile.py` 0.8k, `doppler.py` 0.8k,
`linearize.py` 0.5k), tests 26.9k (1366 tests, 725 under `spy/tests/compiler` each run on 3
backends), C runtime `spy/libspy` 3.2k (+ vendored Ryu 2.0k, walloc), stdlib written in SPy 3.4k
(`stdlib/*.spy`) · Research-grade alpha by Antonio Cuni (PyPy): editable install only, Linux/macOS
CI only, no `try/except`, no refcounting, no heap classes; the ideas are mature, the coverage is not.

All paths below are relative to `research/prior-art/repos/spy/` unless they start with `compiler/` (LotML).

## Architecture

The AST is the only IR; each pass rewrites it and stamps a `LoweringStage`
(`spy/ast.py:39-52`: parsed → astcompiled → redshifting → redshifted → linearized), and every node
class declares the stages it is valid in (`@astnode(spec)`, `assert_valid_at`, `spy/ast.py:137-180,272`).

| Stage | Module · key types | What it does |
|---|---|---|
| pyparse | `spy/magic_py_parse.py:1-35` | rewrites `var x`/`const x` to `var·x` (U+00B7 is an identifier char) with tokenize/untokenize, then CPython's `ast.parse` |
| parse | `spy/parser.py` `Parser` | CPython AST → SPy AST, stage `parsed` |
| symtable | `spy/analyze/scope.py` `ScopeAnalyzer`, `spy/analyze/sym.py:111` `Symbol` | scopes, `varkind` var/const, storage (direct/cell) |
| astcompile | `spy/astcompile.py:1-9` `ASTCompiler` | names → `NameLocalDirect`/`NameOuterCell`/…; `for` → `while`; cached as pickled `.spyc` (`spy/analyze/importing.py:26`, `SPYC_VERSION = 10`) |
| import | `spy/vm/modframe.py` `ModFrame.run` | **executes the module top level at compile time**: `def`/`class`/generics become live `W_*` objects in `SPyVM._globals_w`, keyed by FQN (`spy/vm/vm.py:108-136,210-216`) |
| execute (interp) | `spy/vm/astframe.py:1205` `ASTFrame` | tree-walks the astcompiled body of a red function |
| redshift | `spy/vm/vm.py:227-257`, `spy/doppler.py:120` `DopplerFrame(ASTFrame)` | partial evaluation of every red function until none is left astcompiled; the old `W_ASTFunc` is replaced (`spy/doppler.py:149-186`) |
| linearize | `spy/linearize.py:1-59` | makes left-to-right evaluation explicit for C |
| cwrite | `spy/backend/c/cbackend.py:160-185`, `cwriter.py`, `cmodwriter.py` | one `.c/.h` per `.spy` module, structs topo-sorted into `spy_structdefs.h` (`cbackend.py:261-299`) |
| compile | `spy/build/ninja.py`, `spy/build/flags.py` | ninja + `cc` / `zig cc` / `emcc` |

**How `run` and `build` share.** Everything up to and including the evaluator is shared — not only
the frontend. `DopplerFrame` *is* an `ASTFrame` that overrides `eval_expr` to also compute a residual
("shifted") expression for each node (`spy/doppler.py:383-451`; the "ping-pong" docstring at
`:393-420` traces `return 1 + 3`). Operator resolution, typing and implicit conversions are the same
code in both modes: `vm.call_OP` (`spy/vm/vm.py:915-932`) calls a blue OPERATOR such as `OP.w_ADD`
(`spy/vm/modules/operator/binop.py:281-294`), which returns a `W_OpSpec`, which `typecheck_opspec`
turns into a `W_OpImpl` with explicit conversions (`spy/vm/typechecker.py:37-133`). The interpreter
executes the opimpl (`vm.eval_opimpl`, `spy/vm/vm.py:967-994`); doppler records it
(`spy/doppler.py:453-461`) and emits `Call(FQNConst(opimpl))` (`spy/doppler.py:477-496`).
OPERATORs are blue, so `fast_call` memoizes them in `BlueCache` keyed by (color, static type, blue
value) of each `W_MetaArg` (`spy/vm/vm.py:873-892`, `spy/vm/bluecache.py:19-46`): the interpreter
pays dispatch once per (operator, static types), not per evaluation.

The **runtime is shared too**: the interpreter loads `libspy` compiled to WASI into wasmtime
(`spy/libspy/__init__.py:21-35`) and keeps every `str`, `bytes` and `gc_ptr` in that WASM linear
memory (`spy/vm/str.py:14-27,30-41`, `spy/vm/modules/unsafe/mem.py:50-84`). `float → str` is one C
function for both modes (`spy/vm/primitive.py:394`, `spy/libspy/src/str.c:224-226`: "exported so
the interpreter and generated C share one formatter"). `list`, `dict`, `str` methods are SPy code
over `unsafe.gc_ptr` (`stdlib/_list.spy:17-90`), run by the interpreter and compiled to C alike.

**Contrast with LotML.** LotML shares syntax → check → lower and then forks: `lotml-py` writes Python
from the generic IR (`compiler/crates/lotml-py/src/from_ir.rs`), `lotml-llvm` compiles the
monomorphized, counted IR; the two runtimes are separate implementations (`lotml_rt.py` 1086 lines,
`lotml-runtime/c` ~4.9k lines). SPy gets parity by construction and pays with a slow tree-walker;
LotML gets the CPython ecosystem and speed under `run` and pays with parity by testing
(`compiler/crates/lotml-llvm/tests/common/mod.rs`). SPy's reference semantics is its own
interpreter; LotML's is CPython (adr:0025).

## Frontend

- CPython's own parser and indentation, behind the `var·x` hack (`spy/magic_py_parse.py:47-60`);
  first `SyntaxError` wins, no recovery, grammar tied to the host CPython (3.12). PEP 695
  `def f[T]` / `class C[T]` are reused as generics sugar (`spy/vm/astframe.py:380-423,473-518`).
- No incremental engine: a per-module pickle cache, invalidated by mtime and a version constant
  (`spy/analyze/importing.py:26,142-180`). Imports are fully static, resolved before execution
  (`ImportAnalyzer` docstring, `spy/analyze/importing.py:38-50`).
- Scoping has a strict core (`var`/`const` mandatory) and "pythonic" sugar that infers const from a
  single assignment (`docs/src/reference/scoping.md`); a const bound to a blue value is a blue local
  (`spy/vm/astframe.py:112-131`).
- Inspection is first-class: `spy pyparse|parse|scopes|redshift|build --no-compile`, `--colorize`
  prints each node red/blue from `vm.ast_color_map` (`spy/doppler.py:217-219`), an HTML AST viewer
  (`spy/backend/html.py`), an SPy-source pretty printer of the redshifted program
  (`spy/backend/spy.py`), and SPdb, a source-level debugger for the interpreter
  (`spy/vm/debugger/spdb.py`).

LotML's hand-written tolerant parser and Salsa (adr:0009) are ahead; nothing to take here.

## Semantics and types

- **There is no checker pass.** Typing is abstract interpretation inside the evaluator: a red
  `W_MetaArg` carries only its static type during redshift, a blue one also its value
  (`spy/vm/opspec.py:60-100`). Doppler's error modes: `eager` raises, `lazy` turns the statement into
  a `raise` and warns (`spy/doppler.py:192-215`). The interpreter is lazy by nature — a type error
  in a branch never taken never fires — so the tests encode *when* each backend reports an error
  (`spy/tests/support.py:160-167,275-307`).
- **Colors.** Blue = compile time, red = run time. `@blue` functions are memoized; a call is blue
  when the callee is blue, or when it is pure and all arguments are blue — that is the constant
  folder (`spy/vm/vm.py:976-994`). Purity is a hack: every `operator::*` but `raise`, plus a
  whitelist (`spy/vm/function.py:223-243`). Calling a blue function with red arguments is a type
  error (`spy/vm/typechecker.py:105-114`). Closed-over variables are always blue — closures freeze
  their outer values (`spy/vm/astframe.py:828-836`). `raise` takes blue exceptions only
  (`spy/vm/modules/operator/raiseop.py:31-40`).
- **Generics are blue functions returning types**: `@blue.generic def list(T)` builds a `@struct`
  per `T` (`stdlib/_list.spy:17-60`); memoization *is* monomorphization, and the FQN gets a human
  alias `list[i32]` (`spy/vm/vm.py:880-891`). Bodies are checked per instantiation (C++ template
  semantics): an error shows inside the generic, for one `T`.
- **Metafunctions**: `@blue.metafunc` methods receive MetaArgs and return an `OpSpec`, e.g.
  `list.__getitem__` picks `_getitem_int` or `_getitem_slice` from the index's static type
  (`stdlib/_list.spy:94-101`); `__convert_from__` is the user hook for implicit conversion
  (`stdlib/_list.spy:72-76`).
- **Dispatch is always on static types**, "to preserve the same semantics between interp and
  compile" (`spy/vm/opspec.py:65-67`, `spy/vm/modules/operator/__init__.py:1-36`); `object + object`
  is a type error. `dynamic` dispatches on the runtime type, interpreter only
  (`spy/tests/compiler/test_dynamic.py:9`; "XXX this is wrong",
  `spy/vm/modules/operator/opimpl_dynamic.py:23`).
- Refused or missing: `try/except` (every `raise` is a panic, `ROADMAP.md:108`), heap classes (only
  value `@struct`s), `*args` in C (`spy/backend/c/context.py:178-185`). Value vs reference storage is
  a per-type category (`spy/vm/object.py:564-581`); `is` is refused between value types
  (`spy/vm/modules/operator/binop.py:582-608`). `int` is `i32`, `float` is `f64`
  (`docs/src/llmem.md:39-40`); the interpreter wraps fixed-width integers with `fixedint`
  (`spy/vm/primitive.py:4`). `/` on integers returns `f64`, as LotML's adr:0007.

LotML checks the whole program before any backend, never lazily, and checks bounded generics once
at definition (`docs/wiki/pages/type-system.md:76`) — both better for LLM-written code than SPy's
per-instantiation and run-time-in-interp errors.

## IR and passes

- `astcompile` — name resolution into storage-specific `Name*` nodes, `for`→`while`, desugaring
  (`spy/astcompile.py:1-9`).
- `redshift` (doppler) — per red function: a blue subexpression becomes `Const`/`StrLiteral`/
  `FQNConst` (`spy/doppler.py:37-103,463-475`); assignments to blue locals vanish
  (`:240-242,280-287`); every operator becomes a direct call to its opimpl with explicit
  `Convert` args (`:477-560`, `ArgSpec` in `spy/vm/opimpl.py`); pure conversions of constants fold
  (`:427-446,541-555`); list/dict literals become `new()` + `_push` chains (`:618-701`); `auto`
  locals get their inferred type (`:234-272`); the closure is emptied because every non-local is
  now a constant (`:168-171`); `assert_fully_typed` after each statement (`:195-197`).
- `@force_inline` — single-tail-return callees inlined with alpha-renaming into the caller's
  `FrameInfo` (`spy/force_inline.py`, `spy/doppler.py:498-523`).
- `linearize` — removes `BlockExpr`, spills side-effecting operands to temporaries left to right,
  lowers `and`/`or` with hoisted statements to `if` (`spy/linearize.py:1-59`); pure calls are not
  spilled (`spy/linearize.py:356`).
- No optimizer of its own beyond folding; release C is `-O3 -flto` (`spy/build/flags.py:83-86`).

LotML's lowering already names every intermediate (`compiler/crates/lotml-ir/src/ir.rs:1-2`), so
`linearize` has no counterpart to build. The real difference is the IR's operation vocabulary:
LotML has `Binary(BinOp, a, b, Ty)` plus `Rt { op: Builtin }` (`compiler/crates/lotml-ir/src/ir.rs:10-13,453-530`)
and each backend matches on `(op, Ty)`; SPy's residual program has no operator nodes at all, only
calls to named opimpls (`spy/backend/c/cwriter.py:435-438` refuses a surviving `BinOp`).

## Backend and toolchain

- C99 text through a tiny C AST (`spy/backend/c/c_ast.py`, `--std=c99`, `spy/build/config.py:37-41`).
  FQN mangling `mod::dict[i32, f64]::foo#0` → `spy_mod$dict__i32_f64$foo$0`, using `$` (accepted by
  gcc/clang/MSVC, not standard) and knowingly collision-prone (`spy/fqn.py:362-399`).
- **The builtin table is a naming convention.** `fmt_expr_Call` (`spy/backend/c/cwriter.py:570-633`)
  inlines a fixed list of opimpls as C operators (`FQN2BinOp`, `:450-561`), gives `IRTag`-tagged
  builtins (struct make/getfield, pointer ops, memops; `spy/vm/irtag.py`) special forms, and calls
  everything else by `fqn.c_name`; libspy provides that name, often as a `#define` alias
  (`spy/libspy/include/spy/str.h:112,137`). A missing runtime function is a C error
  (`-Werror=implicit-function-declaration`, `spy/build/flags.py:80`).
- Types: `Context.w2c` (`spy/backend/c/context.py:127-160`) — scalars to C scalars, `str` →
  `spy_StrObject *`, structs by value. `#line SPY_LINE(...)` directives (`cwriter.py:102-129`);
  falling off a non-void function is `abort()` (`cwriter.py:66-73`); `main` wrapper turns argv into
  `list[str]` (`spy/backend/c/cmodwriter.py:155-213`).
- Globals: only `i32` and NULL pointers can be emitted; any other prebuilt constant is a WIP error
  (`cmodwriter.py:255-271`, `cwriter.py:379-399`) — "serialization of the live image" is open
  (`ROADMAP.md:102`).
- Toolchain: native `cc`; `native-static` and `wasi` use `python -m ziglang cc` (zig from PyPI,
  `spy/getzig.py`); emscripten `emcc` (`spy/build/flags.py:91-103`); `-fvisibility=hidden -fPIC`
  (`flags.py:25-30`). Output kinds: exe, WASI "testlib" reactor with exports, `py-cffi`
  (`spy/build/config.py:17-35`).
- Windows: unsupported. CI is ubuntu + macos (`.github/workflows/tests.yml:30`), libspy builds with a
  Makefile that shells `which`, the native CC is the literal `cc`; only `_MSC_VER` guards in
  `spy/libspy/include/spy.h:43-48,82-88`.
- WASM: WASI via zig, emscripten with a JS FFI (`spy/vm/modules/jsffi.py`); the whole compiler runs
  in the browser on Pyodide (`playground/`).

LotML: textual LLVM IR compiled by `clang`, found through `LOTML_CLANG`, `PATH`, `Program Files`
(`compiler/crates/lotml-llvm/src/driver.rs:28-63`).

## Runtime

- `libspy`: mostly `static inline` C in headers (`spy/libspy/include/spy/operator.h`, 768 lines), a
  static `libspy.a` per target × build type × output kind (`spy/libspy/Makefile`).
- Memory: `spy_GcAlloc` is `malloc` and leak by default, Boehm `GC_MALLOC` with `--gc=bdwgc`
  (`spy/libspy/include/spy/gc.h:14-22`). `gc_ptr[T]`/`raw_ptr[T]`, `gc_ref`/`raw_ref`
  (`spy/vm/modules/unsafe/ptr.py:90-116`); in debug builds pointers are fat and carry a length for
  bounds checks (`spy/libspy/include/spy/unsafe.h:96,274`). Refcounting is roadmap only
  (`ROADMAP.md:175-180`).
- Strings: `{size_t length; int32_t hash; gc_ptr_u8 utf8}`, header and bytes co-allocated
  (`spy/libspy/include/spy/str.h:8-31`); literals are static globals (`cwriter.py:341-370`).
- Collections: SPy code; `_ListImpl` wraps `gc_ptr[ListData]`, capacity doubling — reference
  semantics, aliasing visible as in Python (`stdlib/_list.spy:20-90`).
- Errors: `spy_panic(etype, msg, file, line)` prints a Rust-style snippet and aborts
  (`spy/libspy/src/debug.c:65-94`); a WASM testlib hands the panic to the host instead, which raises
  `SPyError` (`spy/libspy/__init__.py:110-125,186-198`).
- Arithmetic: Python floor division and modulo (`operator.h:262-279`), saturating `f64 → i32`
  (`operator.h:66-80`), `**` wraps through unsigned arithmetic (`operator.h:630`) — but `+ - *` are
  raw C operators (`cwriter.py:481-494`) with no `-fwrapv`.
- Float repr: vendored Ryu `d2s`/`f2s` plus a normalizer to CPython's repr conventions
  (`spy/libspy/src/str.c:108-236`, `spy/libspy/vendored/ryu/README.md`).
- CPython interop: none at run time. `py-cffi` writes a cffi build script whose `cdef` declares the
  exported functions and whose source `#define`s cffi-friendly names onto mangled ones
  (`spy/backend/c/cffiwriter.py:1-60`) — scalars and arrays only. The plan (`ROADMAP.md:226-266`):
  SPy as a CPython extension ("a better Cython"), optional libpython linking, and then refcounting
  becomes mandatory and maps to `Py_IncRef/Py_DecRef`. `spy/interop.py` exposes redshift as a
  library for other compilers.

LotML already has what SPy plans: RC from day one (adr:0003, adr:0008), checked Python boundaries
(adr:0012), C ABI exports with a header (adr:0024); adr:0023's embedded CPython is superseded by
adr:0025, which is also where SPy's roadmap is heading in reverse.

## Testing and conformance

- One test, every backend: `CompilerTest` is parametrized over `interp`, `doppler`, `C` with a
  pytest mark per backend (`spy/tests/support.py:20-21,119-147`). `self.compile(src)` returns a
  module wrapper and the test calls `mod.foo(1, 2)` and asserts on Python values
  (`spy/tests/support.py:173-266`):
  - `interp` — `InterpModuleWrapper` wraps/unwraps arguments (`spy/backend/interp.py`);
  - `doppler` — redshift, then *interpret the residual program*, which tests the partial evaluator
    apart from the C emitter;
  - `C` — build a WASI testlib with zig, load it in wasmtime, call exports by `fqn.c_name`
    (`spy/tests/wasm_wrapper.py`); no native C compiler needed, panics come back as exceptions.
  - opt-in `native`, `emscripten`, `py-cffi`, `linearize` (`support.py:222-258`).
- Errors are asserted by message plus annotated source snippets (`expect_errors`,
  `spy/tests/support.py:314-360`), eager or lazy per backend.
- Gaps are named in place: `@skip_backends("C", reason="…")` (`support.py:42-73`) — 10 C skips and
  68 `only_interp`/`no_C` in `spy/tests/compiler`, e.g. "C parser uses strtoll and doesn't match
  int() yet".
- Ordering: interp → doppler → C → emscripten (`spy/tests/conftest.py:24-60`); `AGENTS.md` tells
  agents to run `-m interp` first.
- Node-coverage sweep: every source compiled during the session is pushed through the SPy pretty
  printer at session end and fails on an unsupported node (`spy/tests/conftest.py:107-119`,
  `spy/tests/test_backend_spy.py:13-62`).
- Property-based differential testing: random side-effecting expression programs under `interp` and
  `linearize`, comparing results and stdout (`spy/tests/compiler/test_linearize_hypothesis.py:1-10`);
  profiles `default` 5, `stress` 500 examples (`conftest.py:16-18`).
- No differential testing against CPython; mypy strict over the compiler (`spy/tests/test_zz_mypy.py`).

LotML compares whole-program stdout between the Python and LLVM targets, at `-O0` and `-O2`, with a
cell count at exit (`compiler/crates/lotml-llvm/tests/common/mod.rs:96-140`); it has no
property-based tests (no proptest/hypothesis in any manifest).

## Reusable for LotML

| Item | Path in spy | What it gives | LotML crate | License verdict | Effort |
|---|---|---|---|---|---|
| Ryu shortest float digits | `spy/libspy/vendored/ryu/ryu/{d2s.c,d2s_full_table.h,d2s_intrinsics.h,common.h,digit_table.h,ryu.h}` | shortest round-trip digits in one pass; replaces `lt_shortest`'s up-to-17 `snprintf`+`strtod` loop (`compiler/crates/lotml-runtime/c/lotml.c:551-569`) and its dependence on the CRT's rounding | `lotml-runtime` | copy, keeping `LICENSE-Apache2`/`LICENSE-Boost` (Apache-2.0 OR BSL-1.0) | S |
| Ryu → CPython repr normalizer | `spy/libspy/src/str.c:108-203` | Ryu's `1.5E2` to CPython's `150.0`/`1e+16`, `nan`/`inf` | `lotml-runtime` (`lt_buf_f64`, `lotml.c:571-610`) | adapt, MIT, attribution | S |
| Random program generator for differential tests | `spy/tests/compiler/test_linearize_hypothesis.py` | strategies for expressions with side-effecting calls whose output order is observable | `harness/` (hypothesis) or `lotml-llvm/tests` | adapt, MIT | M |
| Cross-backend test shape | `spy/tests/support.py:119-266`, `spy/backend/interp.py`, `spy/tests/wasm_wrapper.py` | one test body, a backend fixture, per-backend marks and named skips | `lotml-llvm/tests/common/mod.rs` | idea | S |
| Node-coverage sweep | `spy/tests/conftest.py:107-119`, `spy/tests/test_backend_spy.py:13-62` | every program the suite compiles is also fed to another backend's emitter | `lotml-llvm` (compile-only, no clang) | idea | S |
| cffi binding writer | `spy/backend/c/cffiwriter.py` | `cdef` + `set_source` for exported functions | `lotml-llvm/src/export.rs` (`--shared`) | adapt, MIT | S |
| Python-semantics `//`, `%` in C | `spy/libspy/include/spy/operator.h:262-279` | — | — | nothing: `lt_floordiv_i64` exists (`lotml-runtime/c/lotml.h:225`) | — |

## Ideas and optimizations worth adopting

1. **Property-based differential testing between the two targets (high).** SPy's hypothesis test
   generates programs whose observable output depends on evaluation order and compares two
   pipelines. LotML's parity suite is hand-written programs; the bugs that matter at the Python/LLVM
   seam are in arithmetic edges (overflow traps, `//` and `%` signs, `int(f64)` truncation, range
   checks of `u8(n)`, float repr), exactly what random inputs find. Shape: a generator writes a
   LotML program whose `main` prints many fuzzed calls (one clang build amortized over hundreds of
   cases), then `run_python` vs `build_and_run` as `common/mod.rs` already does, comparing stdout and
   exit status. Calling `lotml build --shared` exports through ctypes would be finer-grained, but a
   panic stops the process with status 101 (adr:0024), so it needs a subprocess per case — SPy only
   gets per-call panics because its WASM host turns them into exceptions. Touches no ADR; it is a
   new dev dependency (hypothesis or proptest), which is a `docs/stack.md` entry.
2. **One table from (operator, operand types) to a named operation (medium-high).** SPy resolves
   `a + b` once, in a table (`spy/vm/modules/operator/binop.py:15-277`, `multimethod.py:60-74`), to a
   named opimpl with a signature; the residual IR holds only calls, and a backend implements each
   name (C: by naming convention or an inline-operator map). LotML types operators in a hand-written
   `match` (`compiler/crates/lotml-check/src/body.rs:1704-1778`), lowers to `Binary(op, a, b, Ty)`,
   and each backend re-matches on `(op, Ty)` — three places that must agree. A shared table in
   `lotml-check` returning a `Builtin`-like op id, read by the checker and by `lower.rs`, makes a
   missing backend case a missing table entry. Fits adr:0020 (one IR); keep `Binary` for the cases
   LLVM emits inline, as SPy keeps `FQN2BinOp`.
3. **Ryu for `repr(f64)` in the C runtime (medium, S).** Faster float printing in numeric
   benchmarks and identical digits independent of the C library's `snprintf`/`strtod` (on Windows
   the UCRT). Cost: ~2k lines of C in a runtime compiled with every program (adr:0016's single
   translation unit) — prefer Ryu's small-table variant, which SPy does not vendor, if build time
   shows. Consistent with adr:0016's "reproduces CPython's float repr".
4. **Name parity gaps in place and sweep emitters for coverage (medium, S).** SPy's per-test
   `skip_backends("C", reason=…)` makes the LLVM gap a listed, countable set, and its session-end
   sweep pushes every compiled source through another emitter. For LotML: run every program the
   test suites compile through `lotml_llvm::compile_program` without clang and report which IR
   constructs are refused — a cheap gap metric for specs/llvm-parity on machines without clang.
5. **zig as a pip-installable toolchain fallback (low-medium, verify first).** SPy builds WASI and
   static binaries with `python -m ziglang cc` (`spy/getzig.py`, `spy/build/flags.py:91-96`), so a
   `pip install` brings clang, lld, and libcs. For LotML on Windows machines with only MSVC
   (adr:0016's context), `zig cc` could be a fourth place `driver.rs` looks. To verify before
   adopting: that `zig cc` accepts `.ll` input and that its bundled LLVM reads the textual IR LotML
   writes. Touches adr:0021/adr:0025 (clang is named as the driver) — an amendment, not a conflict.
6. **A compile-time folding pass in `lotml-ir` (low).** SPy's blue evaluation folds pure operators
   on constants and pure conversions of literals (`spy/doppler.py:427-446,541-555`). In LotML a fold
   placed in the shared IR reaches both targets, so it cannot break parity the way SPy's does
   (pitfall 3). The gain is small: no top-level constants exist (`reference/lotml.md:38-39`), LLVM
   folds at `-O2`, CPython's peephole folds too; the useful part is turning a literal that cannot
   fit (`u8(300)`) into a compile error in both targets. **Do not adopt** blue functions,
   metafunctions or template-checked generics as user features: they conflict with adr:0004 (Python
   syntax where semantics match; `@blue` has no Python meaning) and with definition-checked bounded
   generics.
7. **Not recommended: running the C runtime inside `lotml run`.** SPy's parity-by-construction trick
   (libspy in WASM under the interpreter) would make the Python target stop being CPython semantics
   and lose native `list`/`dict` speed — it conflicts with adr:0025 (Python target is the reference)
   and adr:0012.

## Pitfalls seen

1. **Interpreter errors are lazy.** A wrong type in an untaken branch passes `spy foo.spy` and
   fails `spy build`; tests must say per backend when an error appears
   (`spy/tests/support.py:160-167,275-307`). LotML's check-first pipeline avoids this — keep it.
2. **Signed overflow is undefined in the C backend, defined in the interpreter.** `+ - *` on `i32`
   are emitted as bare C operators (`spy/backend/c/cwriter.py:481-494`) with no `-fwrapv`
   (`spy/build/flags.py`), while the interpreter wraps (`fixedint`) and `**` wraps on purpose
   (`operator.h:630`, `spy/tests/compiler/test_int.py:331-337`); no test covers `+` overflow. LotML's
   `llvm.*.with.overflow` traps (`compiler/crates/lotml-llvm/src/emit.rs:860`) are the right answer.
3. **Folding moves an error across phases.** Purity is "module is `operator`"
   (`spy/vm/function.py:223-243`), so a constant `1 // 0` raises during redshift in `doppler`/`C` but
   at run time in `interp`.
4. **Executing the top level at compile time creates a heap you then cannot emit.** Non-primitive
   prebuilt constants are WIP errors in C (`cwriter.py:379-399`, `cmodwriter.py:255-271`,
   `ROADMAP.md:102`).
5. **Memory management deferred.** Default is malloc-and-leak, Boehm optional
   (`spy/libspy/include/spy/gc.h:14-22`); refcounting is to be retrofitted later, mandatory once
   CPython interop lands (`ROADMAP.md:175-180,255-266`), on top of reference-semantics lists built
   from raw `gc_ptr`s. LotML decided RC and value semantics first (adr:0003, adr:0008).
6. **Two implementations of each primitive op held together by tests**: Python in
   `spy/vm/modules/operator/opimpl_int.py`, C in `operator.h`; only formatting and memory moved to the
   shared C-in-WASM. Reusing libc diverged from Python: `int(str)` via `strtoll` does not match
   (named skip in `spy/tests/compiler/test_int.py`).
7. **Small emitter holes**: an `f64` constant is written with Python's `str()`
   (`cwriter.py:288-289`), so a folded `inf`/`nan` is not C, while `f32` got the fix (`:290-300`);
   `$` and `.`→`_` mangling can collide (`spy/fqn.py:366-377`); `W_MetaArg` identity used where it
   compares by value ("THIS IS PROBABLY A BUG", `spy/vm/typechecker.py:123-130`).
8. **Link-order and archive traps they documented**: `-lm` after `-lspy` under `--as-needed`
   (`spy/build/config.py:124-141`); `--whole-archive` or the linker drops unused objects the host
   needs (`config.py:77-85`); Ryu's `__uint128_t` path breaks under zig's WASM multivalue ABI
   (`spy/libspy/Makefile:19-22`).
9. **No Windows at all** (`.github/workflows/tests.yml:30`, `cc` hard-coded, Makefile with `which`):
   a reminder that LotML's driver work for MSVC-only machines is a real differentiator.
10. **`dynamic` is interpreter-only and acknowledged wrong** (`opimpl_dynamic.py:23`), the cost of a
    dynamic escape hatch on a statically dispatched core — consistent with adr:0012 rejecting a
    dynamic `PythonObject` type.
