---
status: accepted
---

# 0023 · Native programs load CPython at run time through its stable ABI

## Context

A program that imports a Python module runs only on the Python target: the native targets refuse
it at the import (specs/c-backend R5.3), and adr:0022-lotml-build-makes-a-native-executable-by-default
keeps that refusal until native code can call CPython. The import itself is settled by
adr:0012-python-interop-through-checked-boundaries-and-interface-files — an interface of
signatures returning `T ! PyError`, every value checked and copied at the boundary — so what is
left to decide is how a native executable reaches an interpreter. Three ways exist: link
`libpython` when the program is built, load it when the program runs, or ship an interpreter
inside the executable. Each CPython minor version has its own full C API ABI; the limited API
(`Py_LIMITED_API`) and its stable ABI, `abi3`, are the subset that does not change across
versions, and they do not cover the free-threaded builds 3.13 introduced. The generated code is
LLVM IR calling a C runtime, so the bridge is C in the runtime crate, not Rust.

## Decision

A native program that imports a Python module loads CPython's shared library when it starts,
resolves the functions it calls by name, and uses only the stable ABI of CPython 3.10 and later.
`lotml build` asks the Python it finds — `LOTML_PYTHON`, then `python3`, `python` and `py -3`, as
the Python target does — for its shared library and its module search path, and writes both into
the executable; `LOTML_PYTHON_LIB` overrides the library when the program runs. A program that
imports no Python module neither loads nor needs CPython. A free-threaded build is refused at
build time. Rejected: linking `libpython` at build time, which ties every executable to one minor
version's ABI and puts Python's development files on the machine that builds; shipping an
interpreter, which is the size and maintenance of a Python distribution in every executable; PyO3,
since the calls come from generated code and a C runtime rather than from Rust; and a dynamic
`PythonObject` type, already rejected by adr:0012.

## Consequences

- The bridge calls CPython through function pointers it resolved, so the executable has no
  link-time dependency on Python and runs against any 3.10+ interpreter that has the recorded
  module search path.
- A missing library, or a module that does not import, stops the program before `main` runs,
  naming what it looked for, as a missing C library does under
  adr:0013-c-libraries-through-interfaces-named-c.
- Values cross by conversion and copy, both directions, at the cost adr:0012 already accepted.
- Free-threaded CPython waits for a stable ABI that covers it; until then such an interpreter is a
  build error naming the reason.
