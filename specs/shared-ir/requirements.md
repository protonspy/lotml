---
autonomy: auto
ci: wait
branch: feat/shared-ir
delivery: merged
pr: 30
---

# Shared IR — requirements

## Purpose

The crate `lotml-ir` between the checker and every backend
(adr:0020-one-ir-between-the-checker-and-every-backend): the checked program lowered once into a
typed, structured IR, with counting, reuse and uniqueness hoisting as passes from IR to IR. It is
for the backends — the C backend first, re-based on it here with its output unchanged, then the
LLVM backend (`specs/llvm-backend/`) and the Python backend (`specs/python-on-ir/`) — so that a
rule the language defines past the checker is lowered in one place. Lowering keeps instantiating
generics as it goes, which is the form both native backends read; keeping them generic, with
monomorphization as a pass of its own, is the delta `specs/python-on-ir/` makes, since the Python
backend is the first to need it.

## R1 · The IR

- **R1.1** The IR shall represent every program the checker accepts, except those R3.3 refuses, as functions of statements over typed, named locals, with `if`, loops and `match` as nested blocks.
- **R1.2** The IR shall carry, on every statement, the source span it was lowered from.
- **R1.3** The IR shall name the language's built-in operations and calls into Python and C libraries by what they do, never by a symbol of one backend's runtime.
- **R1.4** When a program is lowered, the IR shall hold one instance of each generic function and type per set of type arguments the program uses it with.
- **R1.5** The IR shall print as text, one statement per line with its span, in the form its tests compare.
- **R1.6** If a pass leaves a local read before it is set, or an operand whose type is not the one its use requires, then the IR's verifier shall report the pass and the statement when a test build runs it after each pass.

## R2 · Lowering once

- **R2.1** The IR shall lower, in one place for every backend, the integer range checks, `/`, `//` and `%` as adr:0007-integer-division-returns-f64 defines them, the copy on entering a `var` or an `inout`, errors as values, and how a `match` takes a value apart.
- **R2.2** When `lotml check` runs, the compiler shall check the program without lowering it to the IR.

## R3 · Native passes

- **R3.1** When a native backend asks for a program, the IR shall insert counts, reuse and uniqueness hoisting into its monomorphic functions, each a pass from IR to IR.
- **R3.2** The native passes shall manage memory as specs/c-backend R3.1, R3.2, R3.4 and R3.5 require.
- **R3.3** If a generic function or type would need native instances without end, then the IR shall refuse it with a diagnostic at its declaration, as specs/c-backend R5.5 defines.

## R4 · The C backend on the IR

- **R4.1** The C backend shall compile from the IR after the native passes, without importing `lotml_syntax::ast`.
- **R4.2** When the parity suite runs, the C target shall report the same row for every program as before the IR existed.
- **R4.3** When the benchmarks run on the machine of a baseline run made just before from the commit preceding the IR, the C target shall take no more than 5% longer than the baseline as the geometric mean of the benchmarks, and no more than 10% longer on any one.

## Out of scope

The Python backend's move onto the IR (`specs/python-on-ir/`), the LLVM backend, new
optimisations, and any change to what a program does. A control-flow graph of basic blocks as
the shared form, rejected by adr:0020; a native emitter builds its own blocks.
