---
autonomy: auto
ci: wait
pr: per-group
merge: manual
status: approved
checksum: e78c6ba37f48c99428a783c49ae6ec1ad9a3e006d56030231c7b2d6402a768fd
---

# Target parity assurance

Make the parity suite a gate instead of a report. Commit a parity floor that CI holds, fuzz the
LLVM target against the Python target, test on Windows, and give the rules that live outside the
IR the same behavior on both targets.

## Why

All 509 corpus programs report the same on both targets, but the suite runs by hand, its result
is all-or-nothing, and it only covers programs somebody wrote. The prior art shows how parity
regresses unnoticed:

- LPython lost 188 of 400 tests to commented-out CMake lines;
- depyler's oracle was a Rust stand-in, not Python;
- plix shipped a broken Windows calling convention behind a `--version`-only CI.

Done when CI fails on a program leaving the parity floor, a two-target fuzzer runs from the
harness, Windows runs the LLVM tests, and recursion depth is specified identically for both
targets. See `docs/wiki/pages/target-parity.md`.

## Paths

- `harness/lotml_harness/experiments/parity.py`
- `harness/results/`
- `compiler/crates/lotml-llvm/tests/`
- `.github/workflows/ci.yml`
- `specs/`

## References

- adr:0020-one-ir-between-the-checker-and-every-backend
- adr:0025-two-targets-python-for-run-llvm-for-build
- `specs/llvm-parity/` — the parity suite this hardens
- `docs/wiki/pages/target-parity.md` — the evidence
- `research/prior-art/studies/pon.md`, `spy.md`, `plix.md`, `pycc.md`, `depyler.md` — the designs borrowed

## Out of scope

- Changing what any program means on either target: a difference the fuzzer finds is a bug in one emitter, fixed under its own task.
- Resource budgets beyond recursion depth (operation counts, memory limits).

## Tasks

- [x] 1.1 (Unit) Commit a parity floor for the corpus suite — the programs reporting `same` and their count — with check, update and diff commands, and refuse to update it from a filtered run
- [x] 1.2 (Unit) Run the parity floor check in CI and fail only when a program leaves the floor or the count drops
  _Depends 1.1_
- [x] 1.3 (Unit) Group the suite's `refused` and `not compiled` programs by message into a committed JSON beside the report
- [x] 1.4 (Unit) Run the parity suite's built executables with a minimal environment, so no variable reaches a program by accident, while the build keeps what finds `clang` (`PATH`, `LOTML_CLANG`, `SystemRoot` on Windows)
- [x] 2.1 (Unit) Build a two-target differential fuzzer in the harness: typed programs from templates and from `lotml dev mutate`, many cases batched into one `main`, the Python target as oracle, verdicts compared by error kind rather than by bytes; each case gets a wall-clock timeout and a throw-away working directory, and the templates generate no imports and no file access
- [x] 2.2 (Unit) Minimize a failing fuzzer case by deleting chunks greedily while it still differs, and write it out as a parity program
  _Depends 2.1_
- [x] 2.3 (Unit) Build the fuzzer's native programs with `LOTML_SANITIZE` and check `lt_live_cells` at exit, so a counting bug shows even when the output agrees
  _Depends 2.1_
- [x] 3.1 (Unit) Run the LLVM tests of `compiler/crates/lotml-llvm/tests/` on a Windows runner in CI, triggered on `push` and `pull_request` and never on `pull_request_target`
- [x] 3.2 (Unit) Read built executables and libraries with a dependency-free PE and ELF reader in tests: a native program imports no libpython (adr:0025), and a `--shared` library exports exactly its C ABI set; the readers bound every read and return an error on a truncated file
- [x] 3.3 (Unit) Derive a conformance matrix from a manifest in which each parity program declares the language rules it proves and the ones it leaves out
- [x] 4.1 (Unit) Specify, as a new spec named recursion-depth, one recursion limit both targets enforce with the same error, and the stack size native programs reserve for it
- [x] 2.4 (Unit) Make the Python target's `**` of a negative finite base to a finite
      fractional exponent panic with `ValueError`, as the LLVM target does, instead of
      returning a complex number an `f64` cannot hold
  _Reason found by the differential fuzzer of 2.1 (seed 7): `(-1.5) ** 0.1` printed a complex on the Python target and panicked on the LLVM target_
- [x] 2.5 (Unit) Make the Python target's `sum` of integers check each partial sum
      against i64 as `+` does, so it traps where the native target and the same
      additions written out trap, instead of checking only the result
  _Reason found by the differential fuzzer of 2.1 (seeds 2, 3, 4, 6): `sum([3, 9223372036854775807, -7])` printed on the Python target and trapped on the LLVM target_

## Done when

- `uv --directory harness run pytest` passes, including tests for the floor and the fuzzer.
- A CI run fails when a program is removed from the floor's passing set without a recorded reason.
- The CI workflow has a Windows job running `cargo test -p lotml-llvm`.
- The recursion-depth spec exists, and `scc validate` exits 0.
