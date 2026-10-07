# starlark-rust — Meta's Starlark (deterministic Python dialect) interpreter: parser, gradual typechecker, optimizing bytecode VM, frozen heaps, LSP

Repo `research/prior-art/repos/starlark-rust` (paths below are relative to it; LotML paths start
`compiler/`) · license Apache-2.0 (`LICENSE`; `starlark/Cargo.toml:11`). The mirrored Go test
corpus comes from starlark-go, which is BSD-3-Clause (`starlark/testcases/eval/go/README.md:1-7`)
· last commit 2026-10-07; release 0.14 on 2026-05-20 (`CHANGELOG.md:47`) · ~157k lines of Rust:

| Crate | Lines | Largest parts |
|---|---|---|
| `starlark` | 119.7k | `values` 48.8k, `eval` 23.0k, `pagable` 13.9k, `typing` 8.5k, `tests` 6.7k, `analysis` 3.2k |
| `starlark_syntax` | 12.3k | |
| `starlark_map` | 8.0k | |
| `starlark_derive` | 7.3k | |
| `starlark_lsp` | 6.6k | |
| `starlark_bin` | 2.9k | |

Maturity: production. Buck2 depends on it (`README.md:16-17`). Development is very active, and
breaking changes to the value and heap API land in nearly every release (`CHANGELOG.md:5-45,76-94`).

## Architecture

There is one pipeline and it ends in an interpreter. There is no AOT target and no native code.

1. **Lex**: `logos` with a hand-written indentation layer
   (`starlark_syntax/src/lexer.rs:93-116,169-255,1219-1234`).
2. **Parse**: hand-written recursive descent plus a Pratt parser for expressions
   (`starlark_syntax/src/syntax/parser_rd.rs:18-30`). The output is `AstModule`, whose statements
   are `StmtP<P: AstPayload>` (`syntax/ast.rs:35,81-96,182,387`): one generic AST that later
   carries resolution payloads.
3. **Validate**: `syntax/validate.rs` and the `Dialect` feature switches
   (`starlark_syntax/src/dialect.rs:39-70`: `enable_def`, `enable_lambda`, `enable_types`,
   `enable_f_strings`, ...).
4. **Scope resolution**: the AST becomes a "CST" with `BindingId`s and resolved slots (local,
   captured, module) (`starlark/src/eval/compiler/scope.rs`, 1,230 lines).
5. **Optional static typecheck**: `starlark/src/typing/`. It runs in `Lint` or `Compiler` mode
   (`typing/mode.rs:21-25`) and only when enabled (`eval/compiler/module.rs:170`;
   `eval/runtime/evaluator.rs:324-325`).
6. **Compile to a tree IR**, `ExprCompiled`/`StmtCompiled` (`eval/compiler/expr.rs`,
   `eval/compiler/stmt.rs:91-95`). Optimizations live in smart constructors (next section).
7. **Bytecode**: a register/slot VM. `BcWriter` tracks the maximum stack size and which locals are
   definitely assigned (`eval/bc/writer.rs:160-227`). It has about 85 opcodes, many of them
   specialized (`eval/bc/opcode.rs:38-124`).
8. **Freeze**: when a module finishes, its reachable values are copied into an immutable frozen
   heap. Each `def` is then **re-optimized against the frozen module**, so module globals become
   constants and calls to them can be inlined (`eval/compiler/def.rs:125-127,942`).

## Frontend

- **Lexer.**
  - Indentation keeps a stack in `indent_levels`, plus a paren depth that suppresses newlines
    inside brackets (`lexer.rs:93-116`).
  - Indentation is measured on the line after each newline (`lexer.rs:169-255`). A dedent to a
    level that matches no enclosing block is an error (`:244-246`). Tabs are rejected outright:
    `tabs * 8` is computed and then `InvalidTab` is returned (`:223-226`). LotML instead treats a
    tab as advancing to the next multiple of 4 (`compiler/crates/lotml-syntax/src/lexer.rs:311-312,457`).
  - Comment-only lines become comment tokens, and comment spans are exposed through
    `AstModule::comments()` (`lexer.rs:196-217`; `CHANGELOG.md:71`), which feeds tooling.
  - F-string expressions are lexed by a lexer mode (`lexer.rs:712`).
- **Parser.** A hand-written RD/Pratt parser replaced LALRPOP and is "roughly 2x faster on
  per-parse microbenchmarks" (`CHANGELOG.md:68-69,146`). Production names mirror the spec grammar
  (`parser_rd.rs:20-25`). The old `parser_lalrpop.rs` remains as a dead file: it is not declared in
  `syntax.rs:26-44`, and its `grammar` module is gone. This is the same choice LotML made
  (adr:0006, hand-written parser).
- **No error recovery.** `parse_module` returns `Result<AstStmt, EvalException>`, so the first
  error ends the parse (`parser_rd.rs:208,1477`), and lexer errors are fatal. The LSP works only on
  files that parse. LotML's parser is tolerant (`lotml-syntax/src/parser.rs:30,100`), which is
  better for agents.
- **No incremental machinery** (no salsa, no rowan). Spans are `u32` offsets in a `CodeMap`
  (`starlark_syntax/src/codemap.rs`). The AST is owned and re-parsed per file.
- `uniplate.rs` (686 lines) provides generic child-visiting for `ExprP`/`StmtP`, which removes
  visitor boilerplate.

## Semantics and types

- **Dynamic language, optional gradual types.** `docs/types.md:3-4` calls the type system "highly
  experimental", and `:74-75` says it is "intended to be a lossy approximation". Types are checked:
  - at run time on calls and returns (`CheckType`, `ReturnCheckType` opcodes,
    `bc/opcode.rs:97,107`);
  - statically by the typechecker;
  - at compile time against `load`ed interfaces.
- **The typechecker is flow-insensitive per binding.** Each binding's type is the union of all its
  assigned expressions, iterated to a fixed point **capped at 100 iterations**; non-convergence is
  recorded as an `Approximation` (`typing/typecheck.rs:61-130`, with a `FIXME` at `:74`).
  - The types are `TyBasic`: `Any`, `StarlarkValue`, `Iter`, `Callable`, `Type`, `List`, `Tuple`,
    `Dict`, `Custom`, `Set` (`typing/basic.rs:43-64`), plus unions.
  - Builtin method types come from an "oracle" fed by the Rust definitions of the natives
    (`typing/oracle/ctx.rs`; `starlark_derive/src/lib.rs:46-121`, `#[starlark_module]`).
  - Each checked module exports an `Interface` of its public names and their types, consumed by
    modules that `load` it (`typecheck.rs:181-189,320-333`).
- **Runtime type matchers are compiled.** A type expression becomes a `TypeCompiled` matcher,
  and a wildcard (`Any`) costs nothing because the check is dropped (`eval/compiler/types.rs:58-86`).
- **Dynamic features are refused by the language itself**: no `while`, no recursion by default,
  no classes, no exceptions. Determinism comes from freezing: mutating a frozen value is a runtime
  error.
- **Nothing to reuse for LotML's checker**, which is static and sound by intent, with inference
  and generics. Starlark's typechecker is a linter-grade approximation, and its own conformance
  run disables it (`starlark/src/tests/go.rs:47-48`, `// TODO(nga): fix and enable`).

## IR and passes

Optimization happens in the tree IR, mostly in *smart constructors*: building a node folds it
when its operands allow, so no separate pass needs ordering.

- **Constant folding of operators** (`eval/compiler/expr.rs:746-771`):
  - It folds only when both operands are builtin values. It **evaluates with the same `eval`
    function the VM uses**, so the folder cannot disagree with the runtime (`:755-757`).
  - It keeps the expression when the operation fails, so the error appears at run time ("If
    comparison fails, let it fail in runtime", `:592-594`).
- **Unary and `not` folding, `if` simplification**: `if not c: a else b` becomes `if c: b else a`;
  `(x; c) ? t : f` becomes `x; (c ? t : f)`; a constant condition picks a branch
  (`expr.rs:773-829`).
- **Purity analysis**: `is_pure_infallible` and `is_pure_infallible_to_bool` (`expr.rs:403-455`),
  used to drop dead pure expressions and to fold truthiness.
- **List literal concatenation**: `[1] + [2]` becomes `[1, 2]`, with an explosion cap of
  `MAX_LEN = 1000` (`expr.rs:488-507,740-744`). This is pinned by the golden
  `starlark/src/tests/bc/golden/constant_folding_list_add.golden`, where the result is
  `ListOfConsts [1, 2]`.
- **Module globals become constants after freeze**: `ExprCompiled::Module(slot)` is replaced by
  the frozen value (`expr.rs:518-526`).
- **Inlining**:
  - Bodies of the form `return <expr>` that touch only their parameters are inlined, with a size
    cap of 100 nodes and no locals, globals or comprehensions (`eval/compiler/def_inline.rs:44-160`).
  - Inlining happens at call sites when the callee is a frozen def (`eval/compiler/call.rs:142-223`).
  - Inlined code keeps *inlined-frame spans*, so a stack trace still names the inlined function
    (`call.rs:216-221`).
  - Functions with type annotations are not inlined, because their runtime checks get in the way
    (`def.rs:600-607`).
- **Speculative execution of pure natives**: a native marked `#[starlark(speculative_exec_safe)]`
  ("no global side effects, should not panic, and should finish in reasonable time",
  `starlark_derive/src/lib.rs:79-82`) is called at compile time when all its arguments are
  constants (`call.rs:225-246`). Also folded: `"..{}..".format(x)` and `%s` with one argument
  (`call.rs:260+`; `expr.rs:700-735`), and enum construction (`call.rs:249-258`).
- **Specialized bytecode** (`bc/opcode.rs:38-124`):
  - `EqConst`, `EqStr`, `EqInt`, `EqPtr`;
  - `ListOfConsts`, `DictOfConsts`, `DictConstKeys` (a literal with constant keys);
  - `CallFrozenDef[Pos]`, `CallFrozenNative[Pos]`, `CallMaybeKnownMethod` (calls to a known
    callee skip generic dispatch);
  - `ReturnConst`, `AddAssign`, `BitOrAssign`.
  - `ListOfConsts` builds the list in one allocation from a constant slice (`bc/instr_impl.rs:967-980`).
- **GC safepoints**: a `PossibleGc` statement is inserted only before *module top-level*
  statements (`eval/compiler/stmt.rs:679-723`). The copying GC therefore never has to scan
  function frames.

## Backend and toolchain

None. It is a bytecode interpreter in-process. Natives are Rust functions registered through
`#[starlark_module]`. There is also a WASM build (`starlark/src/wasm.rs`, `starlark_js_example/`).
Nothing here bears on LotML's LLVM/clang path.

## Runtime

- **Values** are one word, tagged in the low 3 bits (`values/layout/pointer.rs:18-24`):
  `000` frozen pointer, `001` unfrozen pointer, `010` 32-bit inline int, `100`/`101`
  frozen/unfrozen string. Heap values begin with an `AValueHeader` vtable, so every operation
  dispatches dynamically (`values/layout/avalue.rs`, `vtable.rs`).
- **Unfrozen heap**: a `bumpalo` bump arena whose entries are a header plus payload, or a
  forwarding record (`values/layout/heap/arena.rs:18-44`).
  - GC is a two-space copying collector that handles cycles and costs time proportional to live
    data (`docs/gc.md:1-12`).
  - Freezing uses the same move-with-forwarding walk (`docs/gc.md:5-8`).
- **Frozen heaps**: immutable once sealed and shareable between threads. They are kept alive per
  heap, not per value (`values/layout/heap/frozen.rs:62-65,318-323`;
  `heap/owned_frozen.rs:18-20`), and a heap can depend on other frozen heaps
  (`CHANGELOG.md:108`).
  - Since the latest rework, lifetimes are *brands* (`CHANGELOG.md:5-29`;
    `values/layout/heap/branding.rs`).
  - `Value<'v>` is no longer `Send`/`Sync` (`CHANGELOG.md:83`).
- **Strings** cache a lazily computed hash in their header (`values/types/string/str_type.rs:63-74,164`).
  LotML already does this (`lt_str.hash`, `compiler/crates/lotml-runtime/c/lotml.h:439-447`).
- **Dicts and maps**: `starlark_map::SmallMap` is an insertion-ordered map
  (`starlark_map/src/small_map.rs:18-21,68-81`).
  - It stores short hashes next to the keys and builds **no hash index up to 16 entries** (32 on
    nightly with SIMD), tuned on Buck2 workloads (`:59-66`).
  - It is backed by `Vec2`, which keeps keys and values in separate arrays (`starlark_map/src/vec2.rs:18,104`).
- **Errors**: Starlark has no exceptions. `fail()` and runtime errors become `starlark::Error`
  with a call stack, and inlined frames are reconstructed for it.
- **Resource limits**: an instruction count limit and a heap memory limit (`CHANGELOG.md:114-115`).
- **Serialization of frozen heaps to disk ("pagable")**: 13.9k lines (`starlark/src/pagable.rs:18-37`).
  It exists for Buck2's memory, and none of it is relevant to LotML.
- No CPython interop.

## Testing and conformance

- **Golden files**: 151 `.golden` and 29 `.golden.md` files under `starlark/src`.
  - The template is 75 lines with an env-var regeneration switch
    (`STARLARK_RUST_REGENERATE_GOLDEN_TESTS`) and CRLF normalisation for Windows checkouts
    (`starlark_syntax/src/golden_test_template.rs:24-74`).
  - Bytecode goldens pin every optimization, before and after freeze (`starlark/src/tests/bc/golden/*`,
    for example `compr_if_true_clause_on_freeze.golden`).
- **Conformance against another implementation**: the starlark-go test corpus is mirrored at a
  fixed commit (`starlark/testcases/eval/go/`) and run with *line-level exclusions, each with its
  reason* ("We disagree, see test_not_in_unhashable", "We support {} + {}", `starlark/src/tests/go.rs:25-116`).
- **Parser corpus**: real Bazel `.star` files (`starlark_syntax/testcases/parse/*.star`).
- **`assert` module** for in-language tests (`starlark/src/assert/`).
- Profiling modes (statement, heap, bytecode, flame) double as performance tests
  (`eval/runtime/profile/`).
- No fuzzing in the tree.

## Reusable for LotML

| Item | Path in repo | What it gives | LotML crate it maps to | License verdict | Effort |
|---|---|---|---|---|---|
| Golden-file test helper | `starlark_syntax/src/golden_test_template.rs:24-74` | snapshot tests with an env-var regenerate switch and CRLF handling, **no new dependency** | `lotml-ir/tests` (IR text), `lotml-llvm/tests` (`.ll`), `lotml-py` (Python output), `lotml-check` (type dumps) | copy with attribution (Apache-2.0 into MIT: keep the notice) | S |
| Smart-constructor folding pattern | `starlark/src/eval/compiler/expr.rs:746-829`, `403-455` | folding with no pass ordering; fold only when infallible, else keep the node | a new `lotml-ir/src/fold.rs` | idea / adapt (the code is tied to `Value`) | M |
| Small-function inliner | `starlark/src/eval/compiler/def_inline.rs:44-160`, `call.rs:142-223` | criteria (`return expr`, params only, size cap 100) and inlined-frame spans | `lotml-ir` pass before `native()` and before the Python writer | idea (data structures differ) | M |
| Purity flag on builtins | `starlark_derive/src/lib.rs:79-82`, `call.rs:225-246` | which builtin calls may be folded at compile time | `lotml-check/src/builtins.rs` tables | idea | S |
| Small map without an index | `starlark_map/src/small_map.rs:59-81` | no index allocation for maps of ≤16 entries | `lotml-runtime/c/lotml_dict.c` (dict only) | idea (port to C) | M |
| Conformance-with-exclusions harness | `starlark/src/tests/go.rs:25-116` | run an external corpus and list every disagreement with its reason | parity suite (Python target vs LLVM target), and any CPython-derived corpus | idea; the Go `.star` files are BSD-3, not usable as LotML tests anyway | S |
| LSP completion | `starlark_lsp/src/completion.rs` (373 lines), `server.rs:324-393` (`LspContext`) | completion is the one LSP feature LotML lacks (`compiler/crates/lotml/src/lsp.rs:102-208` routes definition, references, hover, symbols, formatting, codeAction, rename) | `lotml/src/lsp.rs` + `lotml-ide` | idea (tied to Starlark's AST and `DocModule`) | M |
| Lexer / parser / codemap | `starlark_syntax/src/*` | — | nothing: LotML's are hand-written, tolerant, and already match its grammar | — | — |
| Typechecker | `starlark/src/typing/*` | — | nothing: gradual and lossy, versus LotML's static checker | — | — |

## Ideas and optimizations worth adopting

1. **Add constant folding and small-function inlining to `lotml-ir`, ahead of both backends.**
   - Why: LLVM already does both at `-O2` for `build`. The Python target, which serves every
     `lotml run`, `lotml test` and RL rollout, gets neither: CPython folds only literals and never
     inlines across functions, and each LotML call adds an overflow check (`i64()`/`check()`,
     `compiler/crates/lotml-py/runtime/lotml_rt.py:165-176`).
   - `lotml-ir` today has no folding or inlining (`compiler/crates/lotml-ir/src/lib.rs:18-27`: own,
     reuse, hoist).
   - How, taking Starlark's rules:
     - Fold in smart constructors, and only on infallible results (`expr.rs:753-761,592-594`).
     - Fold by calling **one** Rust implementation of each IR operation's semantics, so the folder
       cannot drift from either runtime.
     - An overflowing `+`, a division by zero or an out-of-range index is **not folded**. It stays
       as code so it fails at run time, as adr:0021 requires for checked arithmetic.
     - Inline `return <expr>` bodies under a size cap (`def_inline.rs:89-100`), keeping the
       callee's source location on the inlined nodes so a panic still reports where it came from
       (`call.rs:216-221`; LotML's `lt_at` locations).
   - Pin it with golden IR text (as `constant_folding_list_add.golden` does).
   - ADR: fits adr:0020 (one IR for every backend). Folding must reproduce adr:0007 (`/` on ints
     gives `f64`) and adr:0002 (an error value is never folded away).
2. **All-constant collection literals as static cells, copy-on-write.**
   - Starlark spends opcodes on this (`ListOfConsts`, `DictOfConsts`, `DictConstKeys`,
     `bc/opcode.rs:90-94`; one allocation from a constant slice, `instr_impl.rs:967-980`).
   - LotML's LLVM emitter builds every list literal with `lt_list_new` plus one `lt_list_push` per
     element (`compiler/crates/lotml-llvm/src/emit.rs:1907-1914`).
   - LotML can go further than Starlark. Its runtime already has static cells, count 0, "never
     counted or freed" (`lotml.h:4-5,449-455`, used for string literals). A uniqueness check
     treats a count-0 cell as shared (`lotml.h:132-134` tests `== 1`). So a literal of constants
     emitted as a static cell costs no allocation when it is only read, and is copied on its first
     write by the existing copy-on-write path.
   - For dict literals with constant keys, precompute the hashes (`DictConstKeys`).
   - Expected impact: loops that index lookup tables, and `x in [...]` membership tests, both
     common in LLM-written code.
   - ADR: fits adr:0003/0008 (copy-on-write, counting); no conflict.
3. **Dicts with no index table while small.**
   - `lt_dict` always carries `index`/`mask` beside its entries (`lotml.h:667-678`).
   - Starlark's `SmallMap` skips the index up to 16 entries and scans stored short hashes
     (`small_map.rs:59-81`). Iteration order is the entry order either way, so CPython-order parity
     for dicts is untouched.
   - Do **not** apply this to `lt_set`, which mirrors CPython's table slot for slot to reproduce
     its iteration order (`lotml_dict.c:3-5`).
   - Benefit: less memory and one allocation fewer for the many small dicts used as records.
     Measure before keeping it.
4. **A golden-test helper for every intermediate form.** Copy `golden_test_template.rs`, so
   adding no dependency, and use it for:
   - IR text after `lower`, `own`, `reuse` and `hoist`;
   - the `.ll` written for small programs;
   - the Python module written for small programs;
   - the generated `--shared` header.
   Starlark's bytecode goldens are what keep its optimizer honest across hundreds of commits.
5. **A conformance corpus with reasoned exclusions** (`go.rs:43-116`). When LotML imports test
   programs, for example CPython-semantics programs for the parity suite, keep them verbatim at a
   pinned commit. Express every disagreement as an excluded line with its reason in one test file.
   The list of exclusions then *is* the documented divergence. This fits adr:0004 (Python syntax
   where the semantics match): the exclusions are the places where they do not.
6. **One source of truth for builtins, with a purity bit.**
   - Starlark derives a native's parameter spec, documentation and type from its Rust signature
     (`#[starlark_module]`), plus a `speculative_exec_safe` flag the optimizer reads
     (`starlark_derive/src/lib.rs:46-121`).
   - LotML keeps builtins in the checker's tables (`compiler/crates/lotml-check/src/builtins.rs:7-59`),
     the C mapping (`lotml-runtime/src/lib.rs:29`), `lotml_rt.py` and `lotml.h`.
   - One declarative table carrying each builtin's signature, purity and backend symbols would feed
     idea 1 and cut drift.
   - ADR: none needed unless the table becomes a prelude interface file, which would touch adr:0012.
7. **Completion in the language server**, after `starlark_lsp/src/completion.rs`. It is low for
   agents (who use MCP) and useful for humans. No ADR.
8. **Considered and flagged, not recommended now: freeze-style sharing for `parallel` tasks.**
   - Starlark makes shared data immutable in bulk, with heap-level ownership and no per-value
     counting (`frozen.rs:62-65`; `docs/gc.md`).
   - A LotML analogue would move a value handed to tasks into count-0 static cells for the
     duration of `parallel`, which is structured: all tasks join before it returns. That would
     avoid atomic counting on shared cells.
   - **Conflict:** adr:0008 chose "marked shared once, recursively" with atomic counts. Changing it
     needs a measurement and a new ADR.
9. **Rejected: a Starlark-style bytecode interpreter for `lotml run`.**
   - It would give fast startup and in-process resource limits (`CHANGELOG.md:114-115`).
   - **Conflict:** adr:0025 fixes `run` on CPython for the Python ecosystem.
   - The gradual typechecker is likewise not a model for LotML (see type-system), but its
     `Approximation` record (`typecheck.rs:102-107`) is a good habit wherever an analysis gives up.

## Pitfalls seen

- **Parser generator abandoned for speed.** LALRPOP was replaced by a hand-written parser running
  2x faster (`CHANGELOG.md:68-69`). The dead `parser_lalrpop.rs` is still in the tree (not in
  `syntax.rs:26-44`), which is the cost of carrying two parsers through a migration.
- **No error recovery** (`parser_rd.rs:208`), and lexer errors are fatal (`lexer.rs:224-226,244-246`).
  Every file mid-edit is invisible to the LSP. LotML's tolerant parser avoids this; keep it that way.
- **A fixed point that may not converge**: the typechecker caps it at 100 iterations behind a
  `FIXME` (`typecheck.rs:74-107`), and the conformance run disables static typing
  (`go.rs:47-48`). A gradual checker bolted onto a dynamic language never became sound.
- **Optimizer blow-ups need explicit caps**: `MAX_LEN = 1000` for list folding (`expr.rs:489-490`)
  and a 100-node inline budget (`def_inline.rs:91-94`). Any LotML folder or inliner needs the same
  caps from the start.
- **Folding must not change errors.** Starlark folds only builtin types ("to avoid possible
  problems", `expr.rs:753-754`) and leaves failing operations to run time (`:593-594`). Inlining
  must keep both the evaluation order of failures and definite assignment (`call.rs:176-190`:
  arguments may be inlined only when "definitely assigned").
- **Memory-model API churn.**
  - Heap lifetimes were reworked twice in a row: `&'v Heap` became an invariant `Heap<'v>`
    (`CHANGELOG.md:76-79`), and then branded lifetimes removed `FrozenValue` and friends entirely
    (`:5-29`).
  - `Value` lost `Send`/`Sync` (`:83`).
  - A moving GC plus freezing plus unsafe lifetime tricks took years to make "almost completely
    sound" (`:9-10`). LotML's non-moving reference counting with no stored references (adr:0008)
    sidesteps that whole class of problems.
- **GC only at module top level** (`stmt.rs:679-723`). A long-running function never collects. It
  is the same restriction Mun has (see `mun.md`), and it is how an implementation avoids stack
  scanning.
- **Frozen-value semantics leak into conformance**: "cannot insert into frozen hash table — We
  don't actually have freeze" exclusions (`go.rs:88-90,105-107`). A divergence from the reference
  implementation needs to be recorded where the tests are, as they do.
