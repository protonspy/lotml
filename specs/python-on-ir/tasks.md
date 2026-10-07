# Python on IR — tasks

## 1 · The emitter

- [ ] 1.1 (Unit) Write Python from the IR for functions, numbers, strings, control flow, calls and `print`, every node at its statement's span — R1.1, R2.1
  _Depends 3.2_
- [ ] 1.2 (Unit) Write collections, records, sum types, `match`, optionals, results, closures, methods and generics read once — R1.1, R1.2
  _Depends 1.1_
- [ ] 1.3 (Unit) Write calls through Python and C interfaces, the boundary wrappers and `parallel` — R3.1, R3.2
  _Depends 1.2_

## 3 · The generic IR

- [x] 3.1 (Unit) Keep generic functions and types generic in lowering, every use carrying its type arguments — R4.1
- [x] 3.2 (Unit) Split instantiation out of lowering into the `mono` pass, with the refusal of generics that need instances without end, the native targets' parity and benchmarks unchanged — R4.2, R4.3
  _Depends 3.1_

## 2 · The switch

- [ ] 2.1 (Unit) Remove the emitter that reads the syntax tree, then run the backend's tests and the parity suite unchanged — R1.1, R1.3, R2.1
  _Depends 1.3_
