---
autonomy: auto
ci: wait
branch: feat/c-abi-export
delivery: in-progress
---

# Python on IR — requirements

## Purpose

The Python backend compiling from the IR (`specs/shared-ir/`) instead of the syntax tree, as
adr:0020-one-ir-between-the-checker-and-every-backend decides, so that `lotml run` and
`lotml test` execute the same lowering the native targets compile. It is for the development
loop — a model iterating under `run` and `test` — which must not notice the change: same output,
same tracebacks, same boundary with Python.

## R1 · Same program, read from the IR

- **R1.1** The Python backend shall compile every program the checker accepts from the IR before the native passes, without importing `lotml_syntax::ast`.
- **R1.2** The Python backend shall compile each generic function and type once, not once per set of type arguments.
- **R1.3** When the parity suite and the Python backend's tests run, the Python target shall report what it reported before the change.

## R2 · Positions

- **R2.1** When a program run on the Python target panics or raises, the Python backend's program shall name the `.lot` file, line and function, and underline the LotML expression, as it did before the change.

## R3 · The boundary with Python and C

- **R3.1** The Python backend shall keep the checked boundary of adr:0012-python-interop-through-checked-boundaries-and-interface-files in both directions: wrapped functions and a `.pyi` for Python callers, and calls through interfaces returning `T ! PyError`.
- **R3.2** Where a program imports a C library through its `c.<library>` interface, the Python backend shall call it as adr:0013-c-libraries-through-interfaces-named-c defines.

## R4 · The generic IR

- **R4.1** When a program is lowered, the IR shall keep each generic function and type generic, every use carrying its type arguments.
- **R4.2** When a native backend asks for a program, the IR shall make it monomorphic in a pass from IR to IR before the native passes, refusing as specs/shared-ir R3.3 defines a generic that needs instances without end.
- **R4.3** When the parity suite and the benchmarks run, the native targets shall report what they reported, and take no longer than specs/shared-ir R4.3 allows, before the split.

## Out of scope

Making the Python written from the IR read like the Python written from the syntax tree: the
temporaries the IR names show up in it, which adr:0020 accepts. Changing `lotml_rt`'s protocol.
