---
autonomy: auto
ci: wait
---

# Shared IR — requirements

## Purpose

The crate `lotml-ir` between the checker and every backend
(adr:0020-one-ir-between-the-checker-and-every-backend): the checked program lowered once into a
typed, structured IR, with monomorphization, counting, reuse and uniqueness hoisting as passes
from IR to IR. It is for the backends — the C backend first, re-based on it here with its output
unchanged, then the Python backend (`specs/python-on-ir/`) and the LLVM backend
(`specs/llvm-backend/`) — so that a rule the language defines past the checker is lowered in one
place.

## R1 · The IR

- **R1.1** The IR shall represent every program the checker accepts, except those R3.3 refuses, as functions of statements over typed, named locals, with `if`, loops and `match` as nested blocks.
- **R1.2** The IR shall carry, on every statement, the source span it was lowered from.
- **R1.3** The IR shall name the language's built-in operations and calls into Python and C libraries by what they do, never by a symbol of one backend's runtime.
- **R1.4** When a program is lowered, the IR shall keep each generic function and type generic, every use carrying its type arguments.

## R2 · Lowering once

- **R2.1** The IR shall lower, in one place for every backend, the integer range checks, `/`, `//` and `%` as adr:0007-integer-division-returns-f64 defines them, the copy on entering a `var` or an `inout`, errors as values, and how a `match` takes a value apart.
- **R2.2** When `lotml check` runs, the compiler shall check the program without lowering it to the IR.

## R3 · Native passes

- **R3.1** When a native backend asks for a program, the IR shall make it monomorphic and then insert counts, reuse and uniqueness hoisting, each a pass from IR to IR.
- **R3.2** The native passes shall manage memory as specs/c-backend R3.1, R3.2, R3.4 and R3.5 require.
- **R3.3** If a generic function or type would need native instances without end, then the IR shall refuse it with a diagnostic at its declaration, as specs/c-backend R5.5 defines.

## R4 · The C backend on the IR

- **R4.1** The C backend shall compile from the IR after the native passes, without importing `lotml_syntax::ast`.
- **R4.2** When the parity suite runs, the C target shall report the same row for every program as before the IR existed.
- **R4.3** When the benchmarks run, the C target shall take no more than 5% longer on each than in the run recorded before the IR existed.

## Out of scope

The Python backend's move onto the IR (`specs/python-on-ir/`), the LLVM backend, new
optimisations, and any change to what a program does. A control-flow graph of basic blocks as
the shared form, rejected by adr:0020; a native emitter builds its own blocks.
