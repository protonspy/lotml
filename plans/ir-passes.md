---
autonomy: auto
ci: wait
pr: per-group
merge: manual
status: approved
checksum: dd1e184bbe9e14175a51b6f09b2b83c3ab2cc40256245515f0cd894aa9e239aa
---

# IR passes

Give `lotml-ir` the machinery of a pass pipeline and the optimizations the Python target lacks.
The machinery is a visitor, named passes with dumps and timings, pruning of unreachable
functions, and one operator table every stage reads. The optimizations are constant folding and
small-function inlining, done so that they never move an error from run time to compile time.

## Why

LLVM optimizes `lotml build` at `-O2`, but `lotml run`, `lotml test` and every reinforcement-learning
rollout run on the Python target, which gets no general constant folding (`lower.rs` folds only set literals, to fix their slot order) and no inlining. One operator rule is written
three times: in the checker's `match`, in the IR's `Binary`, and in each backend.

The prior art shows both the method and the trap. Starlark folds only results that cannot fail.
SPy and LPython folded errors into compile time and broke parity between their modes.

Done when the passes run by name with dumps and timings, the operator table is the single source,
and folding and inlining pass the parity suite both on and off. See
`docs/wiki/pages/compiler-performance.md`.

## Paths

- `compiler/crates/lotml-ir/src/`
- `compiler/crates/lotml-check/src/body.rs`
- `compiler/crates/lotml-py/src/`
- `compiler/crates/lotml-llvm/src/emit.rs`

## References

- adr:0007-integer-division-returns-f64
- adr:0020-one-ir-between-the-checker-and-every-backend
- adr:0021-compiler-in-rust-with-llvm-as-its-native-code-generator — overflow traps at run time
- `research/prior-art/studies/starlark-rust.md`, `spy.md`, `lpython.md`

## Out of scope

- Compile-time evaluation of arbitrary functions (SPy's redshift): it would report type errors late, against lotml's generics checked at definition.
- A code-generated IR definition: LPython's ASDL generator buys nothing Rust derives do not.

## Tasks

- [ ] 1.1 (Unit) Add a visitor to `lotml-ir` — operands of a statement, mutable, and a walk over nested blocks — and move the existing passes onto it
- [ ] 1.2 (Unit) Run the IR passes from a named list that can dump the IR after any pass and time each one, behind a `lotml dev` flag
  _Depends 1.1_
- [ ] 1.3 (Unit) Drop functions unreachable from `main` and the exports before the LLVM backend emits them
  _Depends 1.1_
- [ ] 1.4 (Unit) Golden-file tests that pin the inferred types of a set of programs and the IR after each pass
  _Depends 1.2_
- [ ] 2.1 (Unit) Resolve operators through one table — operator, left type and right type to an implementation and its result type — read by the checker, the IR lowering and both backends
- [ ] 2.2 (TDD) Fold constant expressions whose result cannot fail, leaving overflow, division by zero and every other error to run time, keeping the folding of set literals in `lower.rs` as it is, with the parity suite run with folding on and off
  _Depends 1.2, 2.1_
- [ ] 2.3 (TDD) Inline functions whose body is `return <expression>` under a size cap, keeping the callee's source position on the inlined code
  _Depends 1.2_

## Done when

- `lotml dev` dumps and times every pass by name.
- The parity suite reports every program `same` with folding and inlining on and off.
- `cargo test`, clippy and `cargo fmt --check` pass.
