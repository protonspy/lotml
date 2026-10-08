---
autonomy: auto
ci: wait
---

# Language effects and contracts

The two language features phase 4 of the roadmap left open: contracts on a function's inputs, and
effects expressed as an `io` capability passed as a parameter, each measured in the harness before
anything more is built on it. Moved here from `plans/lotml-roadmap.md` 5.2 and 5.3, whose gates 0
to 3 are recorded.

## Why

Requirements R22 and R23 (`docs/wiki/pages/requirements-and-roadmap.md`) are the last of the
roadmap's language features, and the evidence differs between them (`docs/wiki/pages/type-system.md`,
`docs/wiki/pages/language-design-evidence.md`). For contracts it is indirect but present: models omit
input guards, and ignore preconditions nobody stated while honouring some that are stated — a
contract states them in the signature, where the checker sees them. For effect annotations there is
none that they help a model write code, so the capability ships only if the harness shows that
marking it changes what models write. R23 says "checked in debug", but R26 already rejects behaviour
that differs by build, and the Python target has no debug mode, so when contracts are checked is
decided in an ADR before they are built. Done when contracts run on both targets, and both
measurements are recorded with their detectable effect computed the way `plans/evaluation-rigor.md`
1.2 sets.

## Paths

- `compiler/crates/lotml-syntax/`
- `compiler/crates/lotml-check/`
- `compiler/crates/lotml-ir/`
- `compiler/crates/lotml-py/`
- `compiler/crates/lotml-llvm/`
- `harness/`

## References

- `plans/lotml-roadmap.md` — tasks 5.2 and 5.3, struck out there and continued here
- `plans/evaluation-rigor.md` — the detectable effect each measurement states before it runs
- `specs/incremental-check/` — an item's check reads no other item's body; `check --prefix` never rejects a completable prefix
- adr:0019-proceed-to-phase-4-past-the-failed-phase-3-gate
- adr:0025-two-targets-python-for-run-llvm-for-build — both targets carry the contract checks
- `docs/wiki/pages/type-system.md` — the sections Effects and Contracts
- `docs/wiki/pages/colorless-concurrency.md` — the same capability mechanism

## Out of scope

- A typed effect system beyond the `io` capability, unless 2.2 shows the marking matters.
- Contracts proved statically.

## Tasks

- [ ] 1.1 (Unit) Write a proposed ADR on contracts: checked in every build on both targets as R26 holds overflow, or in debug only as R23 says, and the keyword — `where`, which Rust, Swift and C# use for compile-time bounds, or `requires`
- [ ] 1.2 (Unit) Add contracts on both targets as the ADR from 1.1 decides, a failed one trapping with its condition and place (R23)
  _Depends 1.1_
- [ ] 1.3 (Unit) Measure whether stated contracts raise the share of guarded inputs in what models write, stating the model list, cost ceiling and detectable effect before the run
  _Depends 1.2_
- [ ] 2.1 (TDD) Add effects as an `io` capability passed as an ordinary parameter and declared in every signature that takes it, inferring only locals within a body, so that an item's check reads no other item's body and `check --prefix` never rejects a completable prefix (R22)
- [ ] 2.2 (Unit) Measure on tasks that perform I/O whether marking the capability changes what models write — compile refusals, pass@1 and tokens, with and without the marking — stating the model list, cost ceiling and detectable effect before the run, and record the result in `docs/wiki/pages/type-system.md`
  _Depends 2.1_

## Done when

- `cargo test --manifest-path compiler/Cargo.toml` passes with tests for a contract that holds and one that fails on both targets, and the parity suite reports every program `same`.
- The ADR from 1.1 is accepted.
- `harness/results/` holds both measurements with their intervals and costs, and `docs/wiki/pages/type-system.md` cites them.
- `scc validate` exits 0.
