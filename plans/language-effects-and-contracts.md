---
autonomy: auto
ci: wait
---

# Language effects and contracts

The two language features phase 4 of the roadmap left open: `where` contracts checked in debug, and
effects expressed as capabilities passed as parameters, measured before anything more is built on
them. Moved here from `plans/lotml-roadmap.md` 5.2 and 5.3, whose gates 0 to 3 are recorded.

## Why

Requirements R22 and R23 (`docs/wiki/pages/requirements-and-roadmap.md`) are the last of the
roadmap's language features. The evidence differs between them (`docs/wiki/pages/type-system.md`):
contracts are kept with no evidence either way, while effect annotations have none that they help a
model write code, so the capability ships only if the harness shows marking it changes what models
write. Done when contracts run on both targets and the effects comparison is recorded, its
detectable effect computed the way `plans/evaluation-rigor.md` 1.2 sets.

## Paths

- `compiler/crates/lotml-syntax/`
- `compiler/crates/lotml-check/`
- `compiler/crates/lotml-ir/`
- `compiler/crates/lotml-py/`
- `compiler/crates/lotml-llvm/`
- `harness/`

## References

- `plans/lotml-roadmap.md` — tasks 5.2 and 5.3, struck out there and continued here
- `plans/evaluation-rigor.md` — the clustered interval and the detectable effect the comparison reports
- adr:0019-proceed-to-phase-4-past-the-failed-phase-3-gate
- adr:0025-two-targets-python-for-run-llvm-for-build — both targets carry the contract checks
- `docs/wiki/pages/type-system.md` — the sections Effects and Contracts
- `docs/wiki/pages/colorless-concurrency.md` — the same capability mechanism

## Out of scope

- A typed effect system beyond the `io` capability, unless 1.3 shows the marking matters.
- Contracts checked in release builds, or proved statically.

## Tasks

- [ ] 1.1 (Unit) Add `where` contracts checked in debug on the Python and LLVM targets, a failed one reporting its condition and place (R23)
- [ ] 1.2 (TDD) Add effects as an `io` capability passed as a parameter, inferred inside a body and annotated only on public signatures (R22)
- [ ] 1.3 (Unit) Measure in the harness whether marking the capability changes what models write, stating the model list and cost ceiling before the run, and record the result in `docs/wiki/pages/type-system.md`
  _Depends 1.2_

## Done when

- `cargo test --manifest-path compiler/Cargo.toml` passes with tests for a contract that holds and one that fails on both targets, and the parity suite reports every program `same`.
- `harness/results/` holds the effects comparison with its interval and cost, and `docs/wiki/pages/type-system.md` cites it.
- `scc validate` exits 0.
