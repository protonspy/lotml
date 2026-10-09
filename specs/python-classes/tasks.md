# Python classes — tasks

## 1 · The interface

- [x] 1.1 (Unit) Parse `class C(B):` blocks of attribute lines and bodyless `fn` signatures in interface mode into `Item::Class`, keeping E0102 at `class` in a program — R1.5
- [x] 1.2 (Unit) Read a class block into the interface — attributes, constructor, methods, static methods and the bases it declares — resolving type names to the interface's classes, with E0221 for a class in a C interface, an undeclared base or a member without `PyError` — R1.5, R2.1
  _Depends 1.1_

## 2 · The checker

- [x] 2.1 (Unit) Make an imported class's name a type, `py.<module>.<Class>`, and type a call to it as its constructor, `C ! PyError` — R2.1
  _Depends 1.2_
- [x] 2.2 (Unit) Type method calls, static method calls and attribute reads through the class and its declared bases as `T ! PyError`, and refuse assigning an attribute — R2.2, R2.3
  _Depends 2.1_
- [x] 2.3 (Unit) Accept a class where a declared base is expected, and refuse it where any other type is — R2.4
  _Depends 2.1_
- [ ] 2.4 (Unit) Refuse operators, comparison, iteration, `len`, printing, formatting and hashing on a class's value, as on a `PyObject` — R2.5
  _Depends 2.1_

## 3 · The boundary

- [ ] 3.1 (Unit) Lower constructors, static methods, method calls and attribute reads to the runtime with the descriptor `["class", module, name]`, and check a returned class with `isinstance`, held in a `PyHandle` — R3.1, R3.2
  _Depends 2.2_
- [ ] 3.2 (Unit) Refuse a function whose types hold a Python class on the native target with E0402 — R3.3
  _Depends 2.1_

## 4 · The binder

- [ ] 4.1 (Unit) Bind a stub's public, non-generic classes: constructor, methods, static and class methods, annotated attributes and properties, and the bases the interface declares — R1.1, R1.2
  _Depends 1.2_
- [ ] 4.2 (Unit) Write a class the interface declares by its name wherever the stub names it, instead of `PyObject` — R1.3
  _Depends 4.1_
- [ ] 4.3 (Unit) Leave out overloaded, generic, private and dunder members, each named with its reason in the interface's comments, and bring the goldens to the new interfaces — R1.4
  _Depends 4.1_

## 5 · The coverage

- [ ] 5.1 (Unit) Count a `class` of the interface as bound typed, and run the coverage report and the binding corpus after the spec, beside `embedded-typeshed` — R4.1
  _Depends 3.1, 4.3_
