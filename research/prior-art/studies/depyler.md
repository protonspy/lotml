# depyler — annotated Python to Rust source transpiler with ownership heuristics and "semantic verification"

Repo `paiml/depyler` · MIT (`LICENSE`; `Cargo.toml:6` and `:59` declare "MIT OR Apache-2.0", but
no Apache text ships — treat as MIT) · last commit 2026-09-26 · v4.1.1 · Rust: 960k lines across
18 crates, of which about 444k are in files named `*test*` and 116k in `generated_contracts.rs`
files; `depyler-core` alone is 641k (its `src/rust_gen/` 267k, 164k of it tests) · Maturity:
very active and very large, but its own parity report measures 0% semantic matches on the checked
corpus (`semantic_parity_report.json:5-11`); headline numbers are compile rates (`README.md:44-49`).

Paths below are relative to `research/prior-art/repos/depyler/` unless they start with `compiler/` (LotML);
a bare file name such as `lib.rs`, `hir.rs`, `borrowing_context.rs`, `ast_bridge/…` or
`rust_gen/…` is in `crates/depyler-core/src/`.
The repository is big; this study read the pipeline, the type and ownership passes, the
verification crate and the tests that claim semantic equivalence, and nothing else.

## Architecture

`DepylerPipeline::transpile` (`crates/depyler-core/src/lib.rs:645-820`):

1. **Parse** with `rustpython-parser` (`lib.rs:1132`; `crates/depyler-core/Cargo.toml` deps).
2. **AST → HIR**: `ast_bridge::AstBridge::python_to_hir` (`ast_bridge.rs`, 4.4k lines, plus
   `ast_bridge/`), into `HirModule` (`hir.rs`; `Type` at 631-667, `FunctionProperties` at
   299-308).
3. **Type recovery**, five overlapping passes, each allowed to fail quietly:
   - `ConstGenericInferencer` (`lib.rs:654-656`);
   - `TypeHintProvider` hints applied when confidence is High/Certain (parameters) or Medium+
     (returns) (`lib.rs:658-722`);
   - call-site propagation `type_propagation::propagate_call_site_types` (`lib.rs:725`,
     `type_propagation.rs:16`);
   - Hindley–Milner `ConstraintCollector` + `TypeConstraintSolver` (`lib.rs:727-749`,
     `type_system/`, 6.5k lines);
   - inter-procedural `unify_module_types`; "Log but don't fail - type conflicts are
     informational" (`lib.rs:751-756`, `type_system/type_unify.rs:737`).
4. **HIR optimizations** (`optimization::optimize_module`, `lib.rs:759`) and `optimizer::Optimizer`
   (`lib.rs:769-770`; walrus hoisting, constant propagation, DCE, CSE — `optimizer.rs:53-79`).
5. **Analyses printed to stderr** when metrics are on: migration suggestions, performance
   warnings, profiling (`lib.rs:772-801`).
6. **Rust generation** `rust_gen::generate_rust_file` (`rust_gen.rs:540`) through `syn`/`quote`
   token streams; per function, generics, lifetimes and parameter borrowing are decided in
   `rust_gen/func_gen_inference.rs:225-265` (lifetimes at 241-245).
7. **Build** (`depyler compile`): a Cargo project is generated, dependencies included
   (`cargo_toml_gen.rs`), and `cargo build` runs; on failure the "Oracle Loop" parses `E0308`
   errors from cargo's stderr, turns them into type overrides and transpiles again
   (`crates/depyler/src/compile_cmd.rs:11-12`, `71-170`, `289-299`).

**Run vs build**: there is no interpreter; `run` is transpile + cargo build + execute. The only IR
is the HIR, and code generation walks it with many context-specific paths (see Pitfalls).

## Frontend

- `rustpython-parser` on Python source; no error recovery of its own, no incremental parsing.
- Directives live in comments, matched by regex: `#\s*@depyler:\s*(\w+)\s*=\s*(.+)`
  (`crates/depyler-annotations/src/lib.rs:469`), e.g. `# @depyler: ownership = "borrowed"`
  (`…/lib.rs:1286-1287`).
- `Symbol` is documented as "an interned string identifier" and is `String` (`hir.rs:5-6`).

## Semantics and types

- **Type-directed lowering**: Python annotations choose Rust types through `TypeMapper`
  (`type_mapper.rs`); `int` defaults to **`i32`** (`IntWidth::I32`, `type_mapper.rs:116`,
  `352-355`; the README's own example emits `fn fibonacci(n: i32) -> i32`, `README.md:107`),
  while `typeshed_ingest.rs:40` maps `int` to `i64`. Python's unbounded integer becomes a fixed
  width that wraps or panics depending on the Rust profile.
- **Unknown types** fall back to a generated dynamic enum, `DepylerValue`, with arithmetic,
  indexing, ordering and string methods implemented on it (`rust_gen/depyler_value_gen.rs:1-40`);
  `Any`/`object` become `serde_json::Value` in the typeshed mapper (`typeshed_ingest.rs:46-47`).
- **Exceptions → `Result`**: `FunctionProperties { can_fail, error_types }` decide the return
  type — one error type gives a concrete error, several give `Box<dyn Error>`
  (`rust_gen/func_gen_inference.rs:178-203`). Python's `raise` set is inferred, not declared —
  the opposite of LotML's explicit `T ! E` (ADR 0002).
- **Aliasing**: depyler keeps Python's reference semantics by mapping a mutated parameter to
  `&mut T` (`borrowing_context.rs:836-840`). LotML chose value semantics with explicit `inout`
  (ADR 0008), so depyler's borrow choices answer a different question.
- **Ownership inference** (`borrowing_context.rs`): a per-parameter usage summary —
  `is_read`, `is_mutated`, `is_moved`, `escapes_through_return`, `is_stored`, `used_in_closure`,
  `used_in_loop`, field and method accesses (`ParameterUsagePattern`, 31-54) — feeds
  `determine_parameter_strategy` (772-845): moved → take ownership (795-801); escapes and same
  type as the return → ownership (803-811); stored → `Rc` (813-818); used in a closure → "be
  conservative" → ownership (820-824); `Copy` → by value (826-829); strings → `&str` when only
  read, `String` otherwise (848-893); mutated → `&mut`; read → `&`. Whether a call *moves* its
  argument is decided by the callee's **name**: `len`, `str`, `sum`, `sorted`, `map`… borrow;
  `append`, `extend`, `insert`, `remove`, `pop`, `sort` take ownership; anything else borrows
  (`function_takes_ownership`, 671-721).
- **Escape analysis** (`escape_analysis.rs`, 2.0k lines: use-after-move, aliasing that needs a
  clone, `analyze_ownership` at 990) is reached only from
  `BorrowingContext::analyze_with_escape_analysis` (`borrowing_context.rs:915-965`), which
  nothing calls; tests are its only callers. A diverging copy of the same modules lives in
  `crates/depyler-analysis/src/` (`borrowing_context.rs`, `escape_analysis.rs`,
  `lifetime_analysis.rs`, `type_hints.rs` all differ from the `depyler-core` copies).
- **Lifetimes**: `LifetimeInference::apply_elision_rules`, else `analyze_function`
  (`rust_gen/func_gen_inference.rs:241-245`, `lifetime_analysis.rs`); `Cow<'static, str>` was
  tried and abandoned for "impossible lifetime constraints" (`borrowing_context.rs:877-884`).

Compared with LotML: `compiler/crates/lotml-ir/src/own.rs` computes counts from liveness over the
IR (header, 1-5) and `reuse.rs` pairs a dead record with the next constructor (1-4, 37-50) —
exact, type-agnostic and per program point, where depyler's is a name-and-shape heuristic per
parameter.

## IR and passes

- HIR (`hir.rs`), a Python-shaped tree with string names. Passes, each one line:
  - `const_generic_inference.rs` — fixed-size arrays from literal lengths.
  - `type_hints.rs` — heuristic hints with confidence levels.
  - `type_propagation.rs` — call-site argument types to callee parameters.
  - `type_system/hindley_milner.rs`, `constraint_collector.rs`, `solver.rs`, `type_unify.rs` —
    unification over the HIR.
  - `optimization.rs` — annotation-driven rewrites.
  - `optimizer.rs` — walrus hoisting, constant propagation, DCE, CSE; inlining disabled: "Inlining
    pass marks functions as 'Trivial' but doesn't inline them, then dead code elimination removes
    assignments, leaving undefined variables" (`optimizer.rs:32-36`).
  - `borrowing_context.rs`, `lifetime_analysis.rs` — parameter passing and lifetimes, used by
    `rust_gen/func_gen.rs:1452`, `1595`.
  - `escape_analysis.rs`, `inlining.rs`, `generic_inference.rs`, `string_optimization.rs` and more
    — several analysis-only, several printed to stderr.
- No CFG, no SSA, no IR verifier; correctness of the output is delegated to `rustc`.

## Backend and toolchain

- Rust source through `syn`/`quote`, then `cargo build` in a generated project
  (`crates/depyler/src/compile_cmd.rs:201-288`); `main` detected by searching the text for `fn
  main()` (`crates/depyler/src/compile_cmd.rs:223`).
- Python modules map to crates: `json` → `serde_json` (`typeshed_ingest.rs:75`) with Cargo
  dependencies generated (`cargo_toml_gen.rs`).
- "Adding `RUSTFLAGS=-D warnings` to convergence cargo build drops rate from 80%+ to ~0%"
  (`CLAUDE.md:67`): the generated code is warning-ridden.
- No JIT, no shared-library story studied; cross-compilation is whatever cargo does.

## Runtime

No runtime crate. Each generated file carries what it needs: `DepylerValue` and its trait impls
when dynamic values appear (`rust_gen/depyler_value_gen.rs`), `Vec`/`HashMap`/`HashSet` for
collections, `Result` for exceptions, Rust's `String` for `str`. Python's `//` is lowered with a
floor correction in one place (`rust_gen/expr_gen/binary_ops.rs:154-174`).

## Testing and conformance

- **The property tests do not consult Python.** `tests/integration/semantic_equivalence.rs`
  generates programs with proptest (51-83) but its "Python" result is computed in Rust with
  `saturating_add`, `saturating_sub`, `saturating_mul`, truncating `/` and Rust's `%`
  (`eval_python_arithmetic`, 193-220) — none of which is Python's semantics — and "compiles" means
  the text contains `fn ` and braces (`verify_rust_syntax`, 188-191). A failed transpile is
  skipped by `if let Ok(...)` (101-114); `test_type_mapping_consistency` asserts only that a
  string literal is non-empty (262-277).
- **Generated quickcheck tests assert nothing**: the body ends in `TestResult::from_bool(true) //
  Add specific checks` (`crates/depyler-verify/src/properties.rs:97-99`).
- **"Proven" is a label**: `type_preservation` is `Proven` when every parameter is annotated
  (`crates/depyler-verify/src/lib.rs:183-205`); `termination` is `Proven` by
  `StructuralInduction` (153-162) from `check_termination`, which inspects only top-level
  statements and ignores recursion and loops nested in `if` (`ast_bridge/properties.rs:51-65`);
  purity ignores calls on the right of an assignment (`ast_bridge/properties.rs:24-48`).
- **Text assertions as "semantic parity"**: `rust.contains("is_true") || rust.contains("is_empty")
  || rust.contains("PyTruthy")` (`crates/depyler-core/tests/depyler_1157_semantic_parity_audit.rs:44-48`).
- **The real differential script** runs CPython and the Rust binary and compares stdout
  (`scripts/semantic_parity.py:1-12`, `38-90`); its checked-in report: 30 checked, 3 compiling, 0
  semantic matches, `"semantic_parity_rate": 0.0` (`semantic_parity_report.json:5-11`).
- A libFuzzer target asserts only that transpiling never panics
  (`fuzz/fuzz_targets/fuzz_target_1.rs`).

## Reusable for LotML

| Item | Path in repo | What it gives | LotML crate | License verdict | Effort |
|---|---|---|---|---|---|
| Program generator for differential tests | `tests/integration/semantic_equivalence.rs:13-83` | the shape of a proptest strategy for programs (functions, assignments, conditionals over int literals) — to be rewritten for typed LotML | `lotml-llvm` tests | idea only; the code is too weak to adapt | M–L |
| "Never panics" fuzz target | `fuzz/fuzz_targets/fuzz_target_1.rs` | a 20-line libFuzzer harness pattern | `lotml-syntax`, `lotml-check`, `lotml-ir` | adapt (MIT; trivial) — new dev dependency, `docs/stack.md` entry | S |
| Parity metric "compile rate × semantic parity" | `scripts/semantic_parity.py:6-8` | a reporting rule for the LLVM target's gap | `harness`, parity suite | idea only | S |
| Parameter usage summary | `crates/depyler-core/src/borrowing_context.rs:31-54` | the list of facts a borrow decision needs (read, mutated, moved, escapes, stored, captured) | `lotml-ir` (`own.rs`) | idea only; LotML computes these exactly on its IR | M |
| Floor division with sign correction | `crates/depyler-core/src/rust_gen/expr_gen/binary_ops.rs:154-174` | already in LotML (`compiler/crates/lotml-llvm/src/emit.rs:980-1010`) | — | nothing new | — |

## Ideas and optimizations worth adopting

1. **Differential fuzzing of the two targets.** Generate well-typed LotML programs (integers at
   the edges of each `IntKind`, `//` and `%` with negative operands, `f64` division by zero,
   strings with slicing, lists with out-of-range indices, `T ! E` paths), run `lotml run` and
   `lotml build`, and compare stdout, stderr class and exit code, the Python target being the
   oracle. depyler shows the generator shape (`tests/integration/semantic_equivalence.rs:51-83`) and, by its
   mistake, the rule: the oracle must be the real reference implementation, never a
   reimplementation in the test (193-220). LotML's parity suite runs hand-written programs only
   (`compiler/crates/lotml-llvm/tests/common/mod.rs:144-160`). Touches ADR 0025 (the parity
   suite closes the LLVM gap) and ADR 0020 (one IR makes a found bug one fix). Effort M–L; the
   largest expected payoff of this study.
2. **Fuzz the frontend for panics.** The tolerant parser, the checker and the IR lowering receive
   arbitrary agent text through the LSP and MCP servers, where a panic kills the session.
   depyler's target is the pattern (`fuzz/fuzz_targets/fuzz_target_1.rs`); LotML's compiler has
   no fuzzing or property tests today. New dev dependency (`cargo-fuzz`/`libfuzzer-sys` or
   `proptest`) needs a `docs/stack.md` line. Effort S.
3. **Report parity, not builds.** "True Success Rate = Compile Rate × Semantic Parity"
   (`scripts/semantic_parity.py:6-8`): for the LLVM target, report programs that build *and*
   match the Python target, per corpus. ADR 0025's consequence ("measured by the parity suite as
   it closes") asks for exactly this number. Effort S.
4. **Read-only parameters as the first borrow candidates — only behind a benchmark.** depyler's
   usage summary (`borrowing_context.rs:31-54`) lists the facts; for LotML a parameter that the
   callee only reads, never stores, returns, mutates or passes to an owning position could be
   passed without the increment at the call and the decrement in the callee. `own.rs` already
   tracks read-only uses. **Flag:** ADR 0008 puts borrow inference after reuse and "only for the
   cases where the benchmark suite shows it pays, since every borrowed parameter is a reuse given
   up"; this is a candidate for that measurement, not a recommendation to build now. Effort M.

## Pitfalls seen

- **A test oracle that is not the reference** (`tests/integration/semantic_equivalence.rs:193-220`): saturating
  arithmetic and truncating division make the "Python" side agree with Rust, so the test cannot
  catch the most common Python-to-native divergence. LotML's parity suite compares against the
  Python target itself; keep it that way for every generated test.
- **Labels instead of proofs** (`crates/depyler-verify/src/lib.rs:153-205`,
  `ast_bridge/properties.rs:24-65`, `crates/depyler-verify/src/properties.rs:97-99`): "Proven" termination and purity from
  checks that miss recursion, nested loops and calls in assignments. A claim the tool cannot back
  should not be reported, least of all to a model that will trust it.
- **Compile rate as the headline** (`README.md:44-49`) while the repository's own parity report
  shows 0 matches (`semantic_parity_report.json:5-11`). A program that builds and prints the
  wrong answer is worse than one that is refused.
- **Two lowerings of one operator disagree**: `//` floors in `rust_gen/expr_gen/binary_ops.rs:154-174`
  and truncates inside comprehensions (`rust_gen/expr_gen_instance_methods/comprehensions.rs:227-228`).
  Context-specific code generation paths drift; ADR 0020's single IR is the defence.
- **Narrowed integers**: `int` → `i32` by default (`type_mapper.rs:116`) and `i64` in another
  component (`typeshed_ingest.rs:40`). LotML's explicit integer kinds with traps on both targets
  avoid both the narrowing and the disagreement.
- **Ownership by function name** (`borrowing_context.rs:671-721`) and conservative fallbacks
  (closures → ownership, stored → `Rc`, 813-824); a 2k-line escape analysis that only tests call;
  diverging copies of the same modules in two crates.
- **The backend compiler as the type checker**: re-transpiling after parsing rustc's `E0308`
  output (`crates/depyler/src/compile_cmd.rs:11-12`, `289-299`) and five inference passes that "log but don't fail"
  (`lib.rs:658-756`). LotML's checker is authoritative, `compiler/crates/lotml-ir/src/verify.rs` checks the IR,
  and `clang` rejecting the IR is reported as a compiler bug
  (`compiler/crates/lotml-llvm/src/driver.rs:204-218`) — keep all three.
- **Passes that break each other**: inlining disabled because DCE then deleted the assignments it
  relied on (`optimizer.rs:32-36`).
- **Tests that assert on generated text** (`depyler_1157_semantic_parity_audit.rs:44-48`) pass
  for code that never runs.
- **Volume over signal**: about 444k lines of test files, much of it coverage "waves"
  (`crates/depyler-core/src/rust_gen/coverage_wave*_tests.rs`), committed
  `rust_gen.rs.phase{5,6,7}.backup` files, and directives in regex-matched comments
  (`crates/depyler-annotations/src/lib.rs:469`). Coverage percentages measured this way say little
  about correctness; LotML's coverage floor should stay a floor, not a target.
