---
autonomy: auto
ci: wait
branch: feat/python-classes
delivery: in-progress
---

# Python classes — requirements

## Purpose

A program imports a Python module and uses its classes typed: `from py.datetime import date`, then
`date(2026, 1, 8)?.isoformat()?`. The binder writes each class a stub defines into the interface,
and the checker types its constructor, methods and attributes, each fallible as every call into
Python is, while the value stays a handle to the Python object
(adr:0034-a-python-class-crosses-as-a-nominal-handle-with-its-declared-members). Today a class is
bound as `PyObject` (adr:0031-a-python-name-no-stub-types-crosses-as-an-opaque-python-value) or
not at all.

## R1 · The interface

- **R1.1** The binder shall write each public, non-generic class a stub defines at module level as a `class` block of the interface, holding its own attributes, constructor and methods, and naming the bases the same interface declares.
- **R1.2** The binder shall write a class's `__init__` (or `__new__`) as its constructor, a function named after the class returning it; a method taking `self` as a method; a `@staticmethod` or `@classmethod` as a function of the class; and an annotated attribute or a `@property` as an attribute.
- **R1.3** The binder shall write the class's type, instead of `PyObject`, wherever a function, method or attribute of the stub names a class the same interface declares.
- **R1.4** If a class or a member of one is overloaded, generic, a dunder method other than the constructor, or named with a word LotML keeps, then the binder shall leave it out and name it, with the reason, in the interface's comments, and shall leave a private one out as it leaves a private function out.
- **R1.5** The checker shall accept a `class` block in a Python interface only; a `class` in a program shall stay refused with E0102, and one in a C interface with E0221.

## R2 · The checker

- **R2.1** When a program imports a class of a Python interface, the checker shall let the program write the class's name as a type, and type a call to it as its constructor, `C ! PyError`.
- **R2.2** The checker shall type a method call on a value of a Python class, and a call `C.f(...)` to a static or class method, as the interface declares the member, each returning `T ! PyError`.
- **R2.3** The checker shall type reading an attribute of a value of a Python class as `T ! PyError`, and refuse assigning to one.
- **R2.4** The checker shall accept a value of a Python class where a base class the interface declares is expected, and refuse it where any other type is.
- **R2.5** If a program uses an operator, a comparison, iteration, `len`, printing, formatting or hashing on a value of a Python class, then the checker shall refuse it, as on a `PyObject`.

## R3 · The boundary

- **R3.1** When a call into Python returns a value declared as a Python class, the runtime shall check it is an instance of that class, and give `Err(PyError)` of kind `TypeError` when it is not.
- **R3.2** The runtime shall hand Python the object Python gave for a value of a Python class, never copied, printed, compared or hashed.
- **R3.3** The native target shall refuse a program holding a Python class's value as it refuses a `PyObject`: at the import with E0401, and with E0402 in any function whose types hold one.

## R4 · The coverage

- **R4.1** The binding coverage report shall count a public class as a public name, bound typed when the interface declares it, and keep a measurement before this spec and one after it.

## Out of scope

- Overloaded members, generic classes and members: `specs/python-overloads/`, `specs/python-generics/`.
- Operators and the other dunder methods, iteration over a Python object, assigning an attribute.
- A class imported into the stub from another module: re-exports are `plans/python-compatibility.md` 3.4.
- Subclassing a Python class in LotML, or passing a LotML function where Python calls back.
