---
autonomy: auto
ci: wait
branch: feat/phase-3-c-backend
delivery: merged
pr: 16
---

# C backend — requirements

## Purpose

Retired (adr:0025-two-targets-python-for-run-llvm-for-build): the C target and its backend are
gone, the runtime it built on stays as `lotml-runtime`, and the native contract below is restated
for the LLVM target by `specs/llvm-parity/`. What follows records the C target as it was built.

lotml's second target (plans/lotml-roadmap.md task 4.1, R18 and R19 of the roadmap): a checked
program compiled to C and built by the platform's C compiler, giving the results the Python target
gives, with memory managed by reference counting as adr:0008-value-semantics-with-reuse-before-borrowing
decides. It is for the phase 3 gate: the same suite green on both targets, within 2× C on numeric
benchmarks.

## R1 · Same semantics as the Python target

- **R1.1** The C backend shall compile every program the checker accepts, except those R5.3 and R5.5 exclude, to C that a C11 compiler builds without diagnostics.
- **R1.2** When a compiled program runs, the C backend's program shall write to standard output exactly what the Python target's program writes for the same input, and exit with the same status.
- **R1.3** When a value is turned into text by `print`, `str` or an f-string, the C backend shall render it as the Python target does: Python's `repr` of floats and strings, records and variants as their dataclass, sets in CPython's iteration order under `PYTHONHASHSEED=0`.
- **R1.4** When the `test` blocks of a module run on the C target, the C backend shall report each one's name and outcome — pass, fail with the compared values, error, panic — in the JSON `lotml test` writes for the Python target.

## R2 · Arithmetic and broken invariants

- **R2.1** The C backend shall check every integer operation and conversion against the range of its type, `/`, `//` and `%` as adr:0007-integer-division-returns-f64 defines them.
- **R2.2** If an integer operation overflows, an index is out of range, a dict key is absent, a divisor is zero, an `assert` fails or `todo()` runs, then the C backend's program shall stop with status 101 and name the `.lotml` file, line and function where it stopped.
- **R2.3** The C backend shall emit `#line` directives so that the C compiler and a debugger refer to the `.lotml` source.

## R3 · Memory

- **R3.1** The C backend shall manage every heap value by reference counting, with no collector, freeing each value when its last reference is dropped.
- **R3.2** The C backend shall count references without atomic operations, except on a value marked shared.
- **R3.3** When a value is handed to a task of `parallel`, the C backend shall mark it shared, once and recursively, so that its counts are updated atomically from then on.
- **R3.4** When a `match` arm takes apart a unique value that is not used afterwards and builds a value of the same size, the C backend shall reuse the first value's memory for the second.
- **R3.5** When a value that is not unique is changed in place by an `append` or by assigning one of its elements or fields, the C backend shall copy it first so that no other binding sees the change.
- **R3.6** When a program finishes, the C backend's program shall have freed every value it allocated, which a test build reports.

## R4 · Concurrency

- **R4.1** When `parallel(tasks)` runs, the C backend shall run each task on a thread of its own, at most 256 at once, and return their results in order.

## R5 · Building and running

- **R5.1** When a `lotml` command is given `--target c`, the C backend shall compile the program, build it with the first C compiler found among `LOTML_CC`, `CC`, `cc`, `gcc`, `clang` and Visual Studio on Windows, and run it or write the executable.
- **R5.2** If no C compiler is found, then the C backend shall stop with a message naming what it looked for.
- **R5.3** If a program imports a Python module, then the C backend shall refuse it with a diagnostic at the import, since the C target has no Python to call.
- **R5.4** Where a program imports a C library through its `c.<library>` interface, the C backend shall call the library's functions directly and link the library.
- **R5.5** If a generic function or type would need C instances without end, then the C backend shall refuse it with a diagnostic at its declaration: a type whose recursion grows its type arguments, or a function past 64 instances or 256 type nodes in one instance's arguments, since monomorphic C has one definition per instance and compiling them all would exhaust memory.

## Out of scope

Borrow inference, a cycle collector (no cycles can form) and a native backend: adr:0008 puts
borrowing behind a measurement, and adr:0001-transpile-to-python-first leaves Cranelift and LLVM for
after the syntax is stable.
