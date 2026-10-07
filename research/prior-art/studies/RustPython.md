# RustPython — a Python 3 interpreter in Rust: bytecode compiler, VM, and an experimental Cranelift JIT

Repo `research/prior-art/repos/RustPython` · MIT (`LICENSE`, "Copyright (c) 2026 RustPython Team"; workspace
`license = "MIT"`, `Cargo.toml:169`) · last commit 2026-10-08 · version 0.6.1, edition 2024
(`Cargo.toml:164-166`). Rust lines by crate: `vm` 173,652 · `stdlib` 64,049 · `common` 60,005
(about 41k of it generated CJK codec tables, `crates/common/src/encodings/cjk/mappings_*.rs`) ·
`codegen` 58,847 (`compile.rs` 44,141, of which ~30k are the test module from line 13734) ·
`host_env` 29,222 · `doc` 27,995 · `capi` 9,219 · `compiler-core` 7,598 · `derive-impl` 7,053 ·
`jit` 3,350 · `compiler` 2,632 · `sre_engine` 2,476 · `wtf8` 2,279 · `literal` 1,143; plus 1,737
vendored CPython `.py` files under `Lib/`. Maturity: a mature, CPython-3.14-tracking interpreter
(runs pip and the CPython test suite in CI); the JIT is explicitly "very experimental"
(`architecture/architecture.md`, "jit" section).

Relevance to LotML: **medium**. It is an interpreter, not a static compiler, and its parser is now
ruff's. What transfers is runtime algorithms written against CPython's exact semantics (float
formatting, rounding, Unicode predicates, sorting, string indexing), a cautionary JIT, and the
conformance-testing set-up.

## Architecture

Source → ruff parser → AST preprocess → symbol table → codegen (CFG + CPython's flowgraph
optimizer) → `CodeObject` → stack VM (with optional JIT per function).

| Stage | Crate / file | Key types |
|---|---|---|
| Parse | external `ruff_python_parser`, **a RustPython fork** published as `rustpython-ruff_python_parser` 0.16.10 (`Cargo.toml:190-194`) | `ruff_python_ast::ModModule` |
| Parse-error normalisation | `crates/compiler/src/lib.rs` (2,632 lines) | `CompileError`, `cpython_parse_diagnostic_override` (`lib.rs:231`) |
| AST preprocess | `crates/codegen/src/preprocess.rs` — port of CPython `ast_preprocess.c`: `%`-format to f-string (`:439`), docstring handling (`:635`), match-constant folding (`:706`) | `preprocess_mod` (`:297`) |
| Symbol table | `crates/codegen/src/symboltable.rs` (3,951 lines), "Inspirational file: CPython symtable.c" (`:1-8`) | `SymbolTable` (`:25`), `CompilerScope` (`:232`), `SymbolScope` (`:266`), `SymbolFlags` (`:301`), `SymbolTableBuilder` (`:1066`), `SymbolTableAnalyzer` (`:604`) |
| Codegen | `crates/codegen/src/compile.rs` (13.5k non-test lines) | `Compiler` (`:175`), `CompileOpts` (`:298`), `compile_program` (`:547`) |
| CFG + optimizer + assembler | `crates/codegen/src/ir.rs` (8,082 lines), a function-by-function port of CPython `flowgraph.c`/`assemble.c` | `CodeInfo::finalize_code` (`:4232`), `optimize_code_unit` (`:4044`), `optimize_cfg` (`:4069`) |
| Bytecode format | `crates/compiler-core/src/bytecode*.rs`, `marshal.rs` | `CodeObject`, `Instruction`, adaptive counters (`bytecode.rs:557-571`) |
| VM | `crates/vm/src/frame.rs` (12,662 lines, 195 mentions of specialization), `object/core.rs` | `PyObject`, `PyInner` (`core.rs:450-469`) |
| JIT | `crates/jit/src/{lib.rs,instructions.rs}` on Cranelift 0.135 (`Cargo.toml:210-212`) | `compile` (`lib.rs:110`), `FunctionCompiler` (`instructions.rs:70`) |

`run` vs `build`: there is no AOT. The interpreter and the JIT share the whole frontend and the
bytecode; the JIT does not get its own IR — it re-reads the **finished bytecode** of one function
(`instructions.rs:217-224`, "de-specialized opcodes with zeroed CACHE entries"). That is the
opposite of LotML's adr:0020 (one IR between checker and every backend): RustPython's "IR" for the
JIT is a stack bytecode with types erased, and the JIT has to re-infer types from annotations.

## Frontend

- **Parser: not theirs any more.** The architecture doc points at ruff (`architecture.md`,
  "The Parser is located in a separate project, ruff_python_parser"), and the workspace pins a
  RustPython-maintained fork "for RustPython public `_ast` metadata" (`Cargo.toml:190-194`).
  Even a Python implementation could not use ruff's parser unmodified. See `ruff.md` for the
  parser itself.
- **Matching CPython's error messages costs a crate.** `crates/compiler/src/lib.rs` re-scans
  source bytes to turn ruff's errors into CPython's: `cpython_parse_diagnostic_override`
  (`:231-317`) re-parses with an appended newline to tell `E_EOF` from `E_LINECONT`, collapses
  "expected an expression" into "invalid syntax" (`:294-306`), and has hand-written byte scanners
  for f-string nesting (`:540-750`), parenthesis depth (`:801`), blank input (`:751`).
- **AST**: ruff's (`ruff_python_ast`, `TextRange` spans, `Name` small strings). Line/column from
  `ruff_source_file` via `SourceFile` (`compiler-core`).
- **No incrementality.** Compile is one shot per module; no salsa.

## Semantics and types

None static. Python semantics are implemented, not checked. Two places compute types anyway:

- `InferredType` in codegen (`compile.rs:811-858`) — literal-kind inference used only to emit
  CPython-identical `SyntaxWarning`s (e.g. calling a literal).
- The JIT takes parameter and return types from `__annotations__`, accepting only `int`, `float`,
  `bool` (`vm/src/builtins/function/jit.rs:47-66`), and refuses `*args/**kwargs`
  (`jit.rs:69-80`).

**Symbol table vs LotML.** RustPython reproduces CPython's two-pass scheme: a builder records
per-scope `SymbolFlags` (`DEF_LOCAL`, `DEF_PARAM`, `DEF_NONLOCAL`, `USE`, `DEF_FREE_CLASS`,
`DEF_COMP_ITER`…, `symboltable.rs:301-330`), then `SymbolTableAnalyzer::analyze_symbol_table`
(`:612`) resolves each name to `Local | GlobalExplicit | GlobalImplicit | Free | Cell`
(`:266-273`), inlines comprehensions per PEP 709 (`inline_comprehension`, `:449`) and handles
PEP 649 annotation scopes (fields `:99-121`). Nearly all of that complexity serves features LotML
removed: `global`, `nonlocal`, classes, late-binding closures, `import *`, annotation scopes
(`reference/lotml.md`, "Not in the language"). LotML resolves names in the checker
(`lotml-check`, `Checked::locals`, `lotml-check/src/lib.rs:83-85`) and computes lambda captures
during lowering (`lotml-ir/src/lower.rs:3007-3018`), which is the right place for a language whose
lambdas capture copies. Note the caller's comparison targets are not symbol tables:
`lotml-ir/src/symbol.rs` is **symbol mangling** for native backends (`lf_`, `lm<len>_`, `li<n>_`,
`ll<n>`), and `lotml-ir/src/hoist.rs` is a **loop-invariant uniqueness hoist** for copy-on-write
lists. Their RustPython analogues are the JIT's per-function export name
`jit_{obj_name}` (`jit/src/lib.rs:72-76`, unique only because each function gets its own
`JITModule`, `lib.rs:115`) and nothing at all (RustPython has no ownership).

## IR and passes

Two levels, both CPython's:

1. **Instruction sequence / CFG** (`ir.rs`): `Block`s (`:1395`), `BlockIdx` (`:279`),
   `ConstantPool` with NaN/-0.0-aware dedup (`:64-236`).
2. **Bytecode** (`CodeObject`, `compiler-core/src/bytecode.rs`).

Passes, all in `crates/codegen/src/ir.rs`, run by `optimize_code_unit` (`:4044-4067`) and
`optimized_cfg_to_instruction_sequence` (`:4103-4130`):

- `check_cfg` (`:3119`) — a jump or exit must end its block.
- `inline_small_or_no_lineno_blocks` (`:3373`) — duplicate tiny exit blocks.
- `remove_unreachable` (`:1865`).
- `resolve_line_numbers` / `propagate_line_numbers` (`:1999`, `:2560`).
- `optimize_load_const` + `optimize_basic_block` (`:2006`) — peephole: `fold_const_unaryop`
  (`:4547`), `fold_const_binop` (`:4588`), `fold_tuple_of_constants` (`:5317`), list-to-tuple
  intrinsic folding (`:5361`); folding is **size-bounded** (`MAX_INT_SIZE` 128 bits,
  `MAX_COLLECTION_SIZE` 256, `MAX_STR_SIZE` 4096, `:40-51`, checked at `:4669-4791`) so `"x" * 10**9`
  is not folded into the binary.
- `swaptimize` / `apply_static_swaps` (`:1693`, `:1774`) — eliminate `SWAP`s.
- `remove_unused_consts` (`:2753`).
- `add_checks_for_loads_of_uninitialized_variables` (called `:4058`).
- `insert_superinstructions` (`:2847`) — fuse `LOAD_FAST LOAD_FAST` etc.
- `mark_warm` / `mark_cold` / `push_cold_blocks_to_end` (`:2939`, `:2970`, `:3024`) — exception
  handlers moved out of the hot path.
- `calculate_stackdepth` (`:2663`), `normalize_jumps` (`:2735`), `optimize_load_fast` (`:2312`,
  borrow-instead-of-incref for locals), jump resolution and line-table emission (`:1045-1300`).

Runtime-side: adaptive specialization/quickening in the VM (`bytecode.rs:557-571`, dict
`keys_version` stamps `vm/src/dict_inner.rs:40-60`).

None of these apply to LotML's IR (typed, register-like, lowered to LLVM which does this work).
`scripts/compare_bytecode.py` checks RustPython's output **instruction for instruction** against
CPython over all of `Lib/` — the goal that explains why `ir.rs` is a literal port.

## Backend and toolchain

- **Interpreter** only for normal execution. Native binary is the interpreter itself; release
  profile `lto = "thin"` (`Cargo.toml:121`), `mimalloc` by default (`Cargo.toml:14`).
- **JIT** (`crates/jit`, opt-in feature `jit`, `Cargo.toml:24`):
  - Triggered by hand: `f.__jit__()` (`vm/src/builtins/function.rs:1226-1240`); later calls take
    the compiled code if arguments type-check, else log and fall back to the interpreter
    (`function.rs:591-603`).
  - Lowering: one Cranelift `Variable` per Python local, typed on first store; a later store of a
    different type aborts with `NotSupported` (`instructions.rs:113-130`). A pre-pass collects every
    jump target (`:226-233`) and creates one Cranelift block per label; fall-through gets an
    explicit `jump` (`:244-266`); an unterminated tail gets `trap` (`:294-302`).
  - Values: `JitValue::{Int(i64), Float(f64), Bool(i8), None, Null, Tuple, FuncRef}`
    (`:25-34`) — no heap objects, no strings, no lists.
  - Calls: only **self-recursion**; `LoadGlobal` of any other name is `NotSupported`
    (`:702-713`), `Call` emits a direct Cranelift `call` (`:533-571`).
  - Errors: `sadd_overflow`/`ssub_overflow` + `trapnz` (`:382-383`, `:839-840`), division by zero
    `trapz` (`:404`). **No trap handler exists** in `vm/src` or `jit/src` (searched for
    `TrapCode`/`SIGILL`), so an overflow in jitted code kills the process instead of raising.
  - `float ** float` is a ~220-line hand-written double-double `ln`/`exp` in Cranelift IR
    (`compile_fpow`, `:1184`; `dd_*` helpers `:844-1180`) rather than a libcall to `pow`.
  - Invocation through `libffi` with a per-call `Cif` (`lib.rs:140-183`).
- **Shared libraries**: `crates/capi` is a `cdylib` (`capi/Cargo.toml:12`) exporting a subset of
  the CPython C-API (`abstract_`, `longobject`, `unicodeobject`, `pyerrors`… `capi/src/lib.rs`),
  tested with `pyo3` `abi3t` (`capi/Cargo.toml:25`). Irrelevant to LotML since adr:0025 removed
  CPython loading from native programs.
- **Windows**: CI shortens `CARGO_HOME` and sets `core.longpaths` (`.github/workflows/ci.yaml:234-239`)
  — path-length failures on Windows are real for deep Rust/C build trees.
- No linker discovery, no cross-compilation of user code (there is no user-code AOT).

## Runtime

- **Object header** (`vm/src/object/core.rs:450-469`): refcount word, vtable pointer, GC bits,
  GC list pointers, type pointer; payload behind. Reference counting plus a cycle collector
  (`vm/src/gc_state.rs`, 1,699 lines) and free-threading machinery (QSBR, `object/qsbr.rs`).
- **Refcount word** (`common/src/refcount.rs:1-60`): flag bits `destructed | published | leaked |
  immortal` + 60-bit count; **immortal objects** make `inc`/`dec` a load and a branch
  (`:20-45`, `make_immortal` `:303`). LotML already has this: static cells with count 0 are never
  counted (`lotml-runtime/c/lotml.h:3-5`, `LT_STR_LITERAL` `:450`).
- **Strings**: WTF-8 (`crates/wtf8`) so lone surrogates are representable; `PyStr { data, hash }`
  (`vm/src/builtins/str.rs:77-80`) with an ASCII/UTF-8/WTF-8 kind. **`Wtf8Index`**
  (`common/src/wtf8_index.rs:1-60`) — PyPy's `UTF8_INDEX_STORAGE`: one 24-byte group per 64 code
  points (0.375 B/code point) makes code-point→byte offset O(1).
- **Dict**: insertion-ordered index table + entries (`vm/src/dict_inner.rs`, "Inspired by" PyPy
  compact dict, `:1-4`). LotML's `lotml_dict.c` already mirrors CPython's dict and set layout
  entry for entry (`lotml_dict.c:1-5`) — nothing to take.
- **Sort**: a Timsort port with galloping (`vm/src/sorting.rs`, `MIN_GALLOP` `:2`,
  `pub fn timsort<T, E, F>(values: &mut [T], is_lt: &mut F) -> Result<(), E>` `:649`), fallible
  comparator.
- **Numbers**: bigint via `malachite-bigint` (`Cargo.toml:249`); math functions via the external
  crate `pymath` 0.2 (`Cargo.toml:270`, used `stdlib/src/math.rs:33-113`; not in this clone,
  license not checked here). Float ops in `common/src/float_ops.rs`: CPython `_float_div_mod`
  (`divmod` `:97`), `round_float_digits` via exact decimal formatting (`:230`) with a comment
  on why multiply-then-round is wrong for 2.675 (`:231-238`), `round_at_power_of_ten` (`:272`).
- **Float repr / format** (`crates/literal/src/float.rs`): `to_string` (`:327`) on Rust's shortest
  formatter **plus a fix-up**: `prefer_cpython_tie_repr` (`:220-260`) — "Rust's shortest float
  formatter can land on the odd-digit neighbour of a rounding tie where round-half-to-even (what
  `repr` uses) picks the even one" — re-checked with exact u128 decimal/binary distance
  (`decimal_distance_to_f64`, `:306`). `format_fixed`/`format_exponent`/`format_general`
  (`:88`, `:113`, `:177`) pad past Rust's precision cap to stay byte-identical.
- **Format mini-language**: `common/src/format.rs` — `FormatSpec` (`:203`), `FormatSpec::parse`
  (`:327`), `format_float` (`:866`), `format_int` over `BigInt` (`:1001`), `format_string`
  (`:1089`), field-name parsing for `str.format` (`:1436`); generic over `Wtf8`.
- **Unicode predicates**: `crates/unicode/src/classify.rs` states each `str.is*` method as a
  Unicode property test on ICU data: `isdigit` = `Numeric_Type ∈ {Digit, Decimal}` (`:31-38`),
  `isspace` = `Zs` or bidi `WS/B/S` (`:50-60`), `isprintable` (`:65-88`); case mapping in
  `case.rs`.
- **Builtins tables**: proc macros. `#[pymodule] mod _bisect { #[pyfunction] fn bisect_left… }`
  with `#[derive(FromArgs)]` argument structs (`stdlib/src/bisect.rs:3-78`); the macro builds the
  module's method table (`derive-impl/src/pymodule.rs:157` `impl_pymodule`, item kinds
  `pyfunction`/`pyattr`/`pyclass` `:114-130`). Classes via `#[pyclass]`/`#[pymethod]`
  (`derive-impl/src/pyclass.rs`, 2,812 lines). Python-level stdlib is vendored CPython source in
  `Lib/`, optionally frozen into the binary as bytecode at Rust build time (`py_compile!`,
  `derive-impl/src/compile_bytecode.rs`).
- **Exceptions**: Python exceptions as `PyBaseExceptionRef` values in `PyResult` — no unwinding.

## Testing and conformance

- **CPython's own test suite**: `Lib/test` (509 entries) run by `rustpython -m test` in CI
  (`ci.yaml:564-570`). Divergences are recorded in the tests themselves as
  `@unittest.expectedFailure` / `skip` with a `TODO: RUSTPYTHON` comment (convention in
  `architecture.md`, "Lib/test"): 629 markers in 150 files today.
- **Snippets, differential by construction**: `extra_tests/snippets/*.py` (246 files) are
  self-asserting scripts; `extra_tests/test_snippets.py:35-62` runs each one under **both**
  CPython and RustPython, so a snippet that encodes a wrong expectation fails on CPython first.
- **Bytecode parity**: `scripts/compare_bytecode.py` (compiles all of `Lib/` with both
  interpreters and diffs instructions).
- **Surface parity**: `scripts/whats_left.py` lists CPython module members RustPython lacks;
  CI checks it still runs (`ci.yaml:636-638`).
- **Rust unit tests**: 723 in `codegen/src/compile.rs`, 32 in `symboltable.rs`, 31 in `ir.rs`
  (count of `#[test]`); symbol-table tests are named `*_like_cpython` (`symboltable.rs:3429-3943`).
- **JIT tests** (`crates/jit/tests/*.rs`) compile Python source with a `jit_function!` macro and
  compare to hand-written expectations — **not** against CPython, see Pitfalls.
- **Benchmarks** against CPython through `pyo3` (`benches/execution.rs`, dev-dependency `Cargo.toml:65`),
  CodSpeed and pyperformance workflows (`.github/workflows/`).
- No fuzzing found in this clone.

## Reusable for LotML

| Item | Path in repo | What it gives | LotML crate | License verdict | Effort |
|---|---|---|---|---|---|
| Format-spec parser | `crates/common/src/format.rs:203-600` (`FormatSpec`, `parse`, `FormatAlign`/`FormatSign`/`FormatType`) | A complete parse of Python's format mini-language with its error cases | `lotml-check` (validate `f"{x:spec}"` statically; today the checker only types the field expression, `lotml-check/src/body.rs:1211-1219`) | adapt (MIT, attribution); drop `Wtf8`→`&str`, `BigInt`→`i64` | S |
| Code-point index | `crates/common/src/wtf8_index.rs:1-120` (`Wtf8Index`) | O(1) code point → byte offset, 0.375 B per code point | `lotml-runtime/c/lotml_text.c` `lt_offset` (`:110-119`, O(n) per index on non-ASCII text) | adapt to C (MIT; also PyPy's design) | M |
| Timsort | `crates/vm/src/sorting.rs:1-738` (`timsort`, `:649`) | Stable sort with runs and galloping; fewer comparisons on partly sorted input | `lotml-runtime/c/lotml_list.c` `lt_merge_sort` (`:348-392`), `lt_sort_indices` (`:397`) | adapt to C (MIT) | M |
| Python `str.is*` definitions | `crates/unicode/src/classify.rs:13-88` | The exact Unicode-property definition of each predicate | `lotml_text.c` `lt_is_space`/`lt_is_digit` (`:123-134`, a hand-listed subset of digit ranges) — generate C tables from UCD with these rules | idea / adapt (MIT) | M |
| Float repr tie fix | `crates/literal/src/float.rs:220-325` | Makes a Ryu/Grisu shortest result agree with CPython on ties | `lotml.c` `lt_shortest` (`:550-569`) only if the snprintf/strtod loop is replaced by a shortest-digits algorithm | adapt (MIT) | S |
| Exact decimal rounding | `crates/common/src/float_ops.rs:230-312` | `round(x, n)` as `_Py_dg_dtoa` | `lt_round_f64` (`lotml.c:869-905`) already does the same | nothing (equivalent) | — |
| Float divmod | `crates/common/src/float_ops.rs:97-126` | CPython `_float_div_mod` | `lt_divmod_f64` (`lotml.c:691-711`) already identical | nothing | — |
| Constant-fold size bounds | `crates/codegen/src/ir.rs:40-51`, `:4669-4791` | Limits that stop folding from bloating output | `lotml-ir` only if it starts folding (today folding is left to clang / CPython) | idea only | S |
| Symbol table / codegen / CFG optimizer | `crates/codegen/src/{symboltable,compile,ir}.rs` | CPython-identical bytecode | — (LotML has no bytecode; scopes are simpler by design) | nothing | — |
| JIT | `crates/jit/src` | Stack-bytecode → Cranelift for int/float/bool | — (adr:0021/0025 reject Cranelift) | nothing | — |
| `pymodule` macros | `crates/derive-impl/src/pymodule.rs` | One declaration → method table | idea for a single builtin table, see Ideas #5 | idea only | — |

## Ideas and optimizations worth adopting

1. **Check f-string format specs at compile time** (`format.rs:327` adapted). Today
   `f"{name:.2f}"` with `name: str` type-checks and fails at run time — `ValueError` on CPython
   under `lotml run`, a panic in `lt_format_*` under `lotml build`
   (`lotml-runtime/c/lotml.h:727-733`). The field's type is known, so parse the spec in
   `lotml-check` and reject spec/type mismatches (`d` on `f64`, `.2` on `int`, `,` on `str`) with
   a fix. Fits adr:0002 (a runtime stop is reserved for broken invariants; this is a static fact).
   No ADR conflict.
2. **O(1) string indexing on non-ASCII text** (`Wtf8Index`). `lt_offset` walks from byte 0 on every
   `s[i]` when `size != length` (`lotml_text.c:110-119`), so `for i in range(len(s)): s[i]` is
   quadratic on any string with one non-ASCII character. Build the side table lazily on the second
   index and cache it in the `lt_str` cell (needs one pointer field in `lt_str`, `lotml.h:441-447`).
   Touches adr:0016 only as a runtime layout change; static literal cells would carry a null table.
3. **Timsort for `sort`/`sorted`**. Same stable result (so Python/LLVM parity cannot change), fewer
   comparisons on the nearly-sorted inputs typical of LLM-written code (append then sort). Pure
   runtime change, no ADR.
4. **Generate the Unicode predicate tables** from UCD using `classify.rs`'s definitions, instead of
   hand-listed ranges (`lt_is_digit`, `lotml_text.c:128-134`, misses e.g. Thai/Lao/Tibetan digits
   that CPython's `isdigit` accepts). The parity suite against the Python target would catch these
   only if a test happens to use such text.
5. **One declarative builtin table** (inspired by `#[pymodule]`/`#[pyfunction]`): LotML today
   states a builtin in `lotml-check/src/builtins.rs` (signatures), `lotml-ir/src/ir.rs:13`
   (`Builtin` enum), `lotml-runtime/src/lib.rs` (`function(op)` C name) and the C/Python runtimes.
   A single table (or macro) generating the checker signature, IR op and runtime symbol removes a
   drift class. Lower impact: `lotml-runtime/src/abi.rs` already reads C signatures from the
   header so one leg cannot drift. No ADR conflict.
6. **Differential snippets that run on the reference too**: RustPython's snippets are run on
   CPython as well as on the implementation (`test_snippets.py:35-62`), so a wrong expectation
   cannot pass. LotML's parity suite already uses the Python target as reference (adr:0025); keep
   every hand-written expected value in native/runtime tests checked against `lotml run` the same
   way.
- **Not recommended — conflicts with ADRs**: a Cranelift JIT for `lotml run` (rejected by
  adr:0021 and adr:0025, "no Cranelift"); embedding RustPython instead of CPython for `lotml run`
  (adr:0025 chose CPython for the ecosystem — numpy, FastAPI — which RustPython cannot load);
  moving runtime pieces into Rust to reuse `float_ops`/`format.rs` directly (adr:0016 and adr:0021
  keep the runtime C, compiled by `clang` with the program; a Rust staticlib would add Rust's std
  to every native executable).

## Pitfalls seen

- **JIT arithmetic is not Python's, and its tests encode the bug.** Integer `//` lowers to
  Cranelift `sdiv` (truncation, `instructions.rs:395`), `%` to `srem` (dividend's sign, `:419`),
  `*` to `imul` with no overflow check (`:414`) while `+`/`-` do check (`:382`, `:839`). The test
  asserts the wrong value: `assert_eq!(modulo(-5, 10), Ok(-5))` (`jit/tests/int_tests.rs:122`);
  Python gives `5`. `floor_div` tests use no negative-by-positive case (`int_tests.rs:80-85`).
  Exactly the failure `methodology.md` describes — tests written from the code. LotML's emitter
  does floor division and modulo correctly (`lotml-llvm/src/emit.rs:979-1015`); keep native
  arithmetic tests sourced from the Python target, never from expected values typed by hand.
- **JIT traps kill the process.** Overflow and division by zero become Cranelift traps with no
  handler, so a Python-level `OverflowError` becomes a crash. LotML's model (panic with location)
  matches what a trap can express; RustPython's does not.
- **Reimplementing libm in IR** (`compile_fpow`, `dd_ln`, `dd_exp`, `instructions.rs:996-1400`) —
  hundreds of lines with precision risk to avoid declaring one external symbol. LotML calls `pow`
  from C (`lotml.c:727-735`); keep it that way.
- **The JIT has to recover types the compiler erased.** It re-reads untyped stack bytecode and
  guesses each local's type from its first store (`instructions.rs:113-130`); any polymorphic
  local aborts compilation. A backend should consume the typed IR (adr:0020), as LotML's does.
- **Adopting another project's parser means forking it and patching its errors.** RustPython
  depends on its own fork of ruff's parser (`Cargo.toml:190-194`) and ~2,600 lines re-scanning
  source to rewrite ruff's diagnostics into CPython's (`crates/compiler/src/lib.rs`). Evidence for
  `ruff.md`'s verdict.
- **Bit-for-bit porting of CPython's compiler** (`ir.rs` names each function after its
  `flowgraph.c` original, e.g. `:4044-4069`) makes every CPython release a porting task; only
  worth it when bytecode identity is the product. LotML's Python target emits source, not
  bytecode — never target bytecode parity.
- **Rust's shortest float formatting differs from CPython on ties** (`literal/src/float.rs:220-225`)
  and multiply-then-round differs from `round()` (`float_ops.rs:231-238`). If LotML ever replaces
  `lt_shortest`'s printf loop (`lotml.c:550-569`, up to 17 `snprintf`+`strtod` per float) with a
  Ryu-style algorithm for speed, it inherits the tie problem and needs the same fix.
- **WTF-8 everywhere** to carry lone surrogates made every string API generic over `Wtf8`
  (`format.rs`, `anystr.rs`). LotML refuses surrogate escapes in literals
  (`lotml-syntax/src/strings.rs:59-61`, `char::from_u32` fails); keep refusing.
- **Windows path length** in CI (`ci.yaml:234-239`).
