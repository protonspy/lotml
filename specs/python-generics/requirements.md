---
autonomy: auto
ci: wait
branch: feat/python-generics
delivery: in-review
pr: 61
---

# Python generics — requirements

## Purpose

A stub's type variables say how a Python function's or class's types depend on what it is given:
`heapq.nlargest` returns a list of what it was handed, a `re.Pattern[str]` matches text. `lotml
bind` writes every type variable as `PyObject` today and leaves out each generic class, 57 of them
in the binding coverage corpus. This binds them as
adr:0036-a-python-type-variable-crosses-as-a-type-parameter-checked-where-a-value-crosses
decides, for a program calling Python through `py.<module>`.

## R1 · Binding functions

- **R1.1** When a stub's function or member uses a type variable with no constraints, the binder shall write it as a type parameter of the signature.
- **R1.2** When a signature uses a constrained type variable that is not its class's, the binder shall write one overload per constraint, in the stub's order of overloads and then of constraints, within the 64 overloads a name keeps.
- **R1.3** The binder shall write no type variable's bound.
- **R1.4** When a stub imports a type variable from `typing`, `typing_extensions` or `_typeshed`, the binder shall read its kind from the typeshed lotml carries.
- **R1.5** If a signature uses a `ParamSpec` or a `TypeVarTuple`, then the binder shall write what uses it as `PyObject`.

## R2 · Binding classes

- **R2.1** When a stub's class is generic, the binder shall write its class block with its type parameters, in the order PEP 484 gives them.
- **R2.2** When a stub names a generic class of the stub, the binder shall write its type arguments, `PyObject` for each it leaves unnamed.
- **R2.3** When a method's `self` is annotated with an instantiation of its class, the binder shall write `self` with that type.

## R3 · Interface

- **R3.1** Where an interface is a Python module's, the checker shall read the type parameters of its functions, its classes and their members.
- **R3.2** If a type parameter of a Python interface has a bound, then the checker shall report E0221.

## R4 · Checking

- **R4.1** When a program calls a generic Python function, constructor or member, the checker shall infer its type arguments as it infers a LotML function's.
- **R4.2** If a type argument inferred for a Python call is a type the boundary does not carry, then the checker shall report E0204.
- **R4.3** The checker shall type a value of a generic Python class with its type arguments, and its members with those arguments substituted.
- **R4.4** When the overloads of a method restrict `self` to an instantiation, the checker shall choose among them by the receiver's type arguments too.

## R5 · Running

- **R5.1** When a program calls a Python function, constructor or member, the compiler shall convert the arguments and check the result by the signature the checker instantiated for that call.
- **R5.2** The runtime shall check a value of a generic Python class by its class alone.

## R6 · Coverage

- **R6.1** The binding coverage report shall read a generic function's or class's name before its type parameters.
- **R6.2** The binding coverage report shall keep the measurement before this spec and the one after under two labels.

## Out of scope

- A bound written as a LotML trait, and variance: type arguments are invariant.
- A type variable's default (PEP 696), which a call that infers nothing does not use.
- Checking a handle's type arguments when it crosses: Python erases them.
- A generic protocol, still a shape rather than a class.
