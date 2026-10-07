---
autonomy: auto
ci: wait
---

# LLVM parity — requirements

## Purpose

The LLVM target compiling everything the C target compiles, with the same results as the Python
target, so that it can become `lotml build`'s default
(adr:0022-lotml-build-makes-a-native-executable-by-default) and the C target can be retired
(adr:0021-compiler-in-rust-with-llvm-as-its-native-code-generator). Once the C target is gone
this spec holds the native contract `specs/c-backend/` holds today; the semantic requirements
below restate that spec's for the LLVM target.

## R1 · Same semantics as the Python target

- **R1.1** The LLVM backend shall compile every program the checker accepts, except one importing a Python module before `specs/python-bridge/` and one that `specs/shared-ir/` R3.3 refuses.
- **R1.2** When a compiled program runs, the LLVM backend's program shall write to standard output exactly what the Python target's program writes for the same input, and exit with the same status.
- **R1.3** When a value is turned into text by `print`, `str` or an f-string, the LLVM backend's program shall render it as specs/c-backend R1.3 defines: CPython's `repr` of floats and strings, records and variants as their dataclass, sets in CPython's order under `PYTHONHASHSEED=0`.
- **R1.4** When `lotml test --target llvm` runs the `test` blocks of a module, the LLVM backend shall report each one's name and outcome in the JSON `lotml test` writes for the Python target.

## R2 · Broken invariants and debugging

- **R2.1** If an index is out of range, a dict key is absent, or a value fails any check specs/c-backend R2.2 lists, then the LLVM backend's program shall stop with status 101 and name the `.lot` file, line and function where it stopped.
- **R2.2** When `lotml run` compiles a program, the LLVM backend shall write line tables that make a debugger refer to the `.lot` source.

## R3 · Memory and concurrency

- **R3.1** The LLVM backend shall manage memory with the counts, reuse and copies the native passes of `specs/shared-ir/` insert, with no collector.
- **R3.2** When `parallel(tasks)` runs, the LLVM backend's program shall run each task on a thread of its own, at most 256 at once, after marking what each task captures shared.
- **R3.3** When a program finishes in a test build, the LLVM backend's program shall have freed every value it allocated, which the test build reports.

## R4 · C libraries

- **R4.1** Where a program imports a C library through its `c.<library>` interface, the LLVM backend shall call the library's functions directly and link the library.

## R5 · The measurements

- **R5.1** When the parity experiment runs, the harness shall run every program of the corpus on the Python, C and LLVM targets and report, for each native target, whether it reports the same as the Python target.
- **R5.2** When the benchmarks run, the harness shall record the LLVM target's times beside the C target's in `harness/results/benchmarks.md`.

## Out of scope

Closing the numeric gap phase 3 recorded (adr:0019-proceed-to-phase-4-past-the-failed-phase-3-gate):
measured here, fixed separately. Retiring the C target, which is plan task 2.5 and leaves R5.1
with two targets.
