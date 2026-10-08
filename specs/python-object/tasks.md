# Python object — tasks

## 1 · The opaque value and its conversion

- [ ] 1.1 (Unit) Add `PyObject` to the checker: a type in programs and interfaces, every operation refused with the note to convert, and accepted where a `PyObject` parameter is from any type the boundary carries — R1.1, R1.2, R1.3
- [ ] 1.2 (Unit) Check `o.value()` against the type its context expects, `T ! PyError`, an error without one, and lower it to the runtime's `convert` — R2.1, R2.2, R2.3
  _Depends 1.1_
- [ ] 1.3 (Unit) Pass a `PyObject` to Python uncopied and copy LotML values part by part, a LotML record marked apart from a Python dataclass, and keep a `PyObject` a call returns as it is — R1.4, R1.5
  _Depends 1.1_

## 2 · Where it stops, and binding with it

- [ ] 2.1 (Unit) Refuse a `PyObject` on the native target, and leave a function that takes or returns one out of a compiled module's exports with a warning — R3.1, R3.2
  _Depends 1.1_
- [ ] 2.2 (Unit) Have `lotml bind` write `PyObject` for a type it has none for, and count those names apart in the binding coverage report, measured under a new label — R4.1, R4.2
  _Depends 1.3_
