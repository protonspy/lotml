---
autonomy: auto
ci: wait
branch: docs/python-compat-group-2
delivery: in-progress
---

# Python object — requirements

## Purpose

A program reaches the Python names a stub cannot type through an opaque value, `PyObject`, and
takes a LotML value out of one only through a conversion the boundary checks
(adr:0031-a-python-name-no-stub-types-crosses-as-an-opaque-python-value). Most of PyPI becomes
callable before a spec types it, and every place an unchecked Python value enters LotML is
written in the source.

## R1 · The opaque value

- **R1.1** The checker shall accept `PyObject` as a type wherever a type is written, in a program and in an interface.
- **R1.2** If a program reads an attribute of a `PyObject`, calls a method on it, applies an operator to it, compares it, iterates it, prints it or formats it, then the checker shall report it and say to convert the value first.
- **R1.3** When a value is passed where a `PyObject` is expected, the checker shall accept a `PyObject` or any value of a type the boundary of adr:0012 carries.
- **R1.4** When a LotML value is given to a Python function as a `PyObject`, the compiled program shall pass Python a copy of it, and when a `PyObject` is, the compiled program shall pass the Python value it holds, uncopied.
- **R1.5** When a Python function declared to return a `PyObject` returns, the compiled program shall keep the returned value as it is.

## R2 · The conversion

- **R2.1** When a program writes `o.value()` on a `PyObject` where a LotML type is expected, the checker shall type it `T ! PyError` for that type `T`.
- **R2.2** If `o.value()` is written where no type is expected, then the checker shall report it and say to annotate the binding.
- **R2.3** When a conversion runs, the compiled program shall check the Python value against `T` as the boundary of adr:0012 checks a value Python returns, give a copy of it, and give `Err(PyError(kind, message))` when it does not fit.

## R3 · Where it is kept out

- **R3.1** If a program the native target compiles holds a `PyObject`, then the compiler shall refuse it, saying the Python target runs it.
- **R3.2** If a function a compiled module exports to Python takes or returns a `PyObject`, then the compiler shall leave it out of the module's exports and warn which function and why.

## R4 · Binding with it

- **R4.1** When `lotml bind` meets a type it has no LotML type for, the binder shall write `PyObject` in its place and bind the function, rather than leave the function out.
- **R4.2** The binding coverage report shall count the public names bound with a `PyObject` apart from those bound typed.

## Out of scope

- Typing a class, an overload or a generic: `specs/python-classes/`, `python-overloads/`, `python-generics/`.
- An overloaded function stays unbound: which signature a call takes is the overloads spec's.
- Attribute or method access on a `PyObject` checked at run time, rejected by adr:0031.
