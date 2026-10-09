# Python generics — tasks

- [x] 1.1 (Unit) Parse a class's type parameters in an interface, `class Pattern[AnyStr]:`, through the formatter and the IDE — R3.1
- [x] 1.2 (Unit) Read the type parameters of a Python interface's functions, classes and members, a bound refused with E0221, an annotated `self` kept — R3.1, R3.2
  _Depends 1.1_
- [x] 1.3 (TDD) Infer a generic Python call's type arguments, refuse one the boundary does not carry, and record each Python call's instantiated signature by its span — R4.1, R4.2
  _Depends 1.2_
- [ ] 1.4 (Unit) Type a generic Python class's value with its arguments in a program, and its methods and attributes with them substituted — R4.3
  _Depends 1.3_
- [ ] 1.5 (TDD) Choose among generic overloads, and among a method's overloads by the receiver's type arguments — R4.4
  _Depends 1.4_
- [ ] 1.6 (Unit) Lower each Python call by its recorded instantiation, the runtime checking a generic class by its class alone, through to the Python target's run — R5.1, R5.2
  _Depends 1.5_
- [ ] 2.1 (Unit) Bind a stub's plain and bounded type variables as type parameters, imported ones read from the embedded typeshed, a `ParamSpec` or `TypeVarTuple` as `PyObject` — R1.1, R1.3, R1.4, R1.5
  _Depends 1.2_
- [ ] 2.2 (Unit) Expand a constrained type variable into an overload per constraint — R1.2
  _Depends 2.1_
- [ ] 2.3 (Unit) Bind a generic class with its type parameters, a stub class named with its arguments, and an annotated `self` — R2.1, R2.2, R2.3
  _Depends 2.1_
- [ ] 3.1 (Unit) Read a generic name in the coverage report, and measure the corpus as `generics` beside `overloads` — R6.1, R6.2
  _Depends 1.6, 2.2, 2.3_
- [ ] 3.2 (Unit) Write generics into the language reference, the guide, the E0221 explanation, the glossary and the wiki — R3.1, R4.3
  _Depends 3.1_
