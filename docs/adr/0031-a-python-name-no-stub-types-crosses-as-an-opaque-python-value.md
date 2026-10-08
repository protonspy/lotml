---
status: accepted
---

# 0031 · A Python name no stub types crosses as an opaque Python value

## Context

adr:0012-python-interop-through-checked-boundaries-and-interface-files binds only what lotml can
type: a parameter or result typed `Any`, a union other than `X | None`, a class, a callable or an
overload leaves its function out, listed in a comment. The binding coverage report
(`specs/binding-coverage/`) measures the result on a fixed corpus: 12.4% of the public names are
bound, 22.4% of the standard library's and 0.8% of PyPI's. numpy, pandas and requests are almost
entirely out of reach, and a function is out even when a single parameter of it cannot be typed.

adr:0012 rejected treating Python values as dynamic, as Mojo and Codon do, because that gives up
the types a stub already has. That rejection stands for the names a stub types. The question here
is what to do with the remainder, which today is nothing: a program cannot call it at all.

## Decision

The owner accepted this on 2026-10-08. A type a stub writes that lotml has no type for is bound
as `PyObject`, an opaque handle to a Python value, and the function is bound instead of left out.
`plans/python-compatibility.md` 2.2 specifies it as `specs/python-object/`.

- **Opaque.** A `PyObject` has no operations of its own: no attribute, no method, no operator, no
  comparison, no iteration, no `print`. It can be stored, passed and returned, and given to a
  Python function whose parameter is `PyObject`.
- **Into Python, as the boundary carries.** A parameter typed `PyObject` takes any lotml value the
  boundary of adr:0012 can carry, copied as the boundary copies it, and the `PyObject` it was given.
- **Out only through a conversion.** A `PyObject` becomes a lotml value only through an explicit
  conversion to a named lotml type, which runs the boundary checks of adr:0012 on it: the integer
  range, the element types, the record's fields, a copy. A conversion that fails is
  `Err(PyError(kind, message))`, so the conversion is fallible like every call into Python. The
  spelling is the spec's.
- **Kept to the Python target.** A program holding a `PyObject` imports Python, which `lotml build`
  already refuses (adr:0025-two-targets-python-for-run-llvm-for-build). A function a compiled
  module exports to Python may not take or return a `PyObject`: the wrapper adr:0012 puts around it
  has nothing to check it against.
- **A typed name stays typed.** A later spec that types a class, an overload or a generic
  (`specs/python-classes/`, `python-overloads/`, `python-generics/`) replaces the `PyObject` it
  covered; the coverage report counts a name bound with a `PyObject` apart from one bound typed.

Rejected:
- Leaving the untyped names out, as today: most of PyPI stays unreachable.
- Every Python value dynamic, as Mojo and Codon have it: it gives up the types a stub has, which
  adr:0012 rejected.
- A gradual `Any` converted implicitly where a lotml type is expected: each implicit conversion is
  a place a call can fail that the program does not show.
- Attribute and method access on a `PyObject`, checked at run time: a dynamic sublanguage inside
  lotml, and the reason a typed binding of classes is a spec of its own.

## Consequences

- A function with one untyped parameter is callable, its `PyObject` parameter taking what the
  program passes; most of the corpus becomes reachable before it becomes typed.
- A program that needs a value out of a `PyObject` writes the conversion and handles its failure,
  so every place an unchecked Python value enters lotml is visible in the source.
- The coverage report gains a column: names reachable through `PyObject` beside names typed.
- `PyObject` is a prelude type of the Python target only, and the native target refuses it as it
  refuses the import that brought it.
