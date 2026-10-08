# Origin imports — tasks

## 1 · The import, the check and the target

- [x] 1.1 (Unit) Import `py.<module>` through the interface of that name, `py` a package in an expression, and a bare import of a Python module E0216 with its fix or its rename — R1.1, R1.2, R2.1, R2.2
- [x] 1.2 (Unit) Write `lotml bind <module>` to `bindings/py.<module>.lotmli`, and call CPython's module with `py.` taken off on the Python target — R1.3, R1.4
  _Depends 1.1_
- [x] 1.3 (Unit) Move the tests, the agent guide and `reference/lotml.md` to `py.<module>` — R3.1
  _Depends 1.2_
