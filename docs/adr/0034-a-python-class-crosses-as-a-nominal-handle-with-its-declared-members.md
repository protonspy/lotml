---
status: proposed
---

# 0034 · A Python class crosses as a nominal handle with the members its interface declares

## Context

adr:0031-a-python-name-no-stub-types-crosses-as-an-opaque-python-value binds a class a stub
names as `PyObject`, and says a later spec types it. The binder writes none of a stub's classes:
`datetime`, `decimal` and `pandas` import and bind no function (`harness/results/bind-on-import.md`),
and the functions that take or return a class reach it only as an opaque value. An interface holds
functions alone (E0221), and LotML has no `class`: a program declares a record with `type` and its
methods in `impl` (E0102).

A Python object is not a LotML value. It has identity, it changes under the program, and its
attributes are read through Python code that can raise. Copying one into a record, as the
boundary of adr:0012-python-interop-through-checked-boundaries-and-interface-files copies lists and
dicts, would give the program a snapshot that no longer is the object its methods act on.

## Decision

A class a stub defines is bound as a nominal type whose values are handles to the Python object,
as `PyObject`'s are, and on which the program may use what the interface declares and nothing
else. `specs/python-classes/` specifies it.

- **Declared in the interface by `class`.** A Python interface may hold `class C(B):` blocks, each
  listing the class's annotated attributes (`year: int`), its constructor as a function named after
  the class (`fn date(year: int, month: int, day: int) -> date ! PyError`), its methods (with
  `self`) and its static and class methods (without). `class` stays refused in a program (E0102):
  a Python class is used, not declared. The block mirrors the stub, so a reviewer reads it as the
  stub it came from.
- **Nominal, by its origin.** The type is the class of that module: two classes of one name in two
  modules are two types, and none is a LotML record of the same name. A class is accepted where one
  of its bases the interface declares is expected, and nowhere else.
- **Every read and call can fail.** A constructor, a method, a static method and an attribute read
  are each `T ! PyError`: each runs Python code, and the result is checked at the boundary as a
  function's is. An attribute is never assigned to.
- **A handle, never a copy.** The runtime checks a value declared as a class is an instance of it,
  holds it in a handle it never copies, prints, compares or hashes, and gives Python the object
  Python gave. Operators, comparison, iteration and printing are refused, as on a `PyObject`.
- **What the class spec leaves out.** Overloaded members (`specs/python-overloads/`), generic
  classes and members (`specs/python-generics/`), dunder methods other than the constructor, and a
  base the same interface does not declare; each named in the interface's comments.
- **Kept to the Python target**, as `PyObject` is (adr:0025-two-targets-python-for-run-llvm-for-build).

Rejected:
- A record per class, its fields the attributes, copied at the boundary: a snapshot of a mutable
  object, whose methods could not run on it.
- `type` and `impl` in the interface, as a program writes them: a record's constructor takes its
  fields, a Python class's takes its `__init__` parameters, and `impl` has no place for an
  attribute or a base.
- Attribute and method access checked at run time on every `PyObject`: the dynamic sublanguage
  adr:0031 refused.
- Attributes read without `PyError`: a property is Python code, and its value crosses the same
  boundary a result does.

## Consequences

- `from py.datetime import date` then `date(2026, 1, 8)?.isoformat()?` checks and runs, and a
  function taking or returning a `date` is typed instead of reaching it as `PyObject`.
- The checker gains a kind of nominal type and member lookup through declared bases; the parser
  gains `class` blocks in interfaces only.
- The binding coverage report counts a class as a public name, typed when the interface declares
  it.
- A class's members a stub overloads or makes generic stay out until their specs land, so most of
  pandas still binds through `PyObject`.
