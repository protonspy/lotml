# Python bridge — tasks

## 1 · Loading CPython

- [ ] 1.1 (Unit) Ask the found Python for its library, search path, version and build at build time, refusing one older than 3.10 or free-threaded — R2.1, R2.4
- [ ] 1.2 (Unit) Load the library and import the program's modules before `main` in the runtime's bridge, stopping with status 101 when either fails, and compile the bridge only into programs that import Python — R2.2, R2.3, R2.5
  _Depends 1.1_

## 2 · Calling

- [ ] 2.1 (TDD) Convert values both ways through the type descriptors, with the Python target's errors for a value that does not fit — R1.3, R1.4
  _Depends 1.2_
- [ ] 2.2 (Unit) Compile calls through Python interfaces on the LLVM target, returning `T ! PyError` — R1.1, R1.2, R1.3
  _Depends 2.1_
- [ ] 2.3 (Unit) Hold the interpreter lock only during each call, and report unreleased Python objects in test builds — R1.5, R1.6
  _Depends 2.2_
- [ ] 2.4 (Unit) Run the corpus programs that import Python modules on the LLVM target against the Python target — R1.1, R1.2, R1.3
  _Depends 2.3_
