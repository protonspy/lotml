---
autonomy: auto
ci: wait
branch: feat/python-overloads
delivery: merged
pr: 60
---

# Python overloads — requirements

## Purpose

A stub declares with `@overload` that one Python function returns a different type for different
arguments: `os.listdir(".")` is a `list[str]`, `os.listdir(b".")` a `list[bytes]`. `lotml bind` leaves
every such function and member out today, 50 public functions and 8 class members of the binding
coverage corpus. This binds them as adr:0035-a-python-overload-crosses-as-ordered-signatures-chosen-at-the-call
decides: the overloads in order, a call typed by the one its arguments fit, for a program calling
Python through `py.<module>`.

## R1 · Binding

- **R1.1** When a stub declares a function or member with `@overload`, the binder shall write each of its overloads under the one name in the order the stub declares them, as it writes a function.
- **R1.2** If an overload cannot be bound or takes a receiver the first does not, then the binder shall leave it and every later overload of the name out, each listed with its reason.
- **R1.3** If an overload's written parameters are an earlier overload's, then the binder shall leave it out, listed as never chosen.
- **R1.4** When a stub declares the overloads of one name in several branches of an `if`, the binder shall bind those of the first branch that declares them.
- **R1.5** If a name has more than 64 overloads, then the binder shall write the first 64 and list the rest as past the limit.

## R2 · Interface

- **R2.1** Where an interface is a Python module's, the checker shall read a function, a constructor or a method declared more than once as one with those overloads, in the order declared.
- **R2.2** If the overloads of a member differ in whether they take `self`, or a name has more than 64, then the checker shall report E0221 and leave the overloads past the first such one out.

## R3 · Calls

- **R3.1** When a program calls an overloaded function, constructor, static method or method, the checker shall check each argument once and give the call the first overload that accepts the arguments by count, keyword and type without passing a value that is not a `PyObject` to a `PyObject` parameter, else the first that accepts them at all.
- **R3.2** If no overload accepts a call's arguments, then the checker shall report E0204, listing the overloads.
- **R3.3** If a program names an overloaded function other than to call it, then the checker shall report E0226.
- **R3.4** When a program calls an overloaded function, constructor, static method or method, the compiler shall convert the arguments and check the result by the overload the checker gave the call.

## R4 · Coverage

- **R4.1** The binding coverage report shall count an overloaded name bound typed when one of its overloads is typed, and reachable otherwise.
- **R4.2** The binding coverage report shall keep the measurement before this spec and the one after under two labels.

## Out of scope

- A program declaring overloads of its own: a LotML name is declared once (E0210).
- Typing a generic overload: it is bound as the binder binds a type variable, a `PyObject`, until
  `specs/python-generics/`.
- Dunder overloads, which operators would call.
