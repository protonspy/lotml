# C backend — tasks

## 1 · The pipeline and the core

- [x] 1.1 (Unit) Find a C compiler and build a C file with the runtime, naming what was looked for when none is found — R5.1, R5.2
- [x] 1.2 (TDD) Lower and emit functions, numbers, `bool`, control flow and `print`, with checked integer arithmetic, `#line` and panics naming the `.lotml` line — R1.1, R1.2, R2.1, R2.2, R2.3
  _Depends 1.1_
- [x] 1.3 (TDD) Compile strings: literals, operators, indexing and slices, methods, f-strings with the format mini-language, `str()` and CPython's float and string `repr` — R1.2, R1.3
  _Depends 1.2_

## 2 · Values and their counts

- [x] 2.1 (TDD) Compile lists, tuples, ranges, comprehensions and the prelude over them — R1.2, R1.3
  _Depends 1.3_
- [x] 2.2 (TDD) Insert counts: owned parameters, moves at the last use, decrements where a value dies, copy before changing a value that is not unique, and a leak report in test builds — R3.1, R3.2, R3.5, R3.6
  _Depends 2.1_
- [x] 2.3 (TDD) Compile records, sum types, `match`, optionals and results with `?`, `fail` and `??` — R1.2, R1.3
  _Depends 2.2_
- [x] 2.4 (TDD) Reuse a unique value's memory in the `match` arm that takes it apart — R3.4
  _Depends 2.3_
- [x] 2.5 (TDD) Compile dicts and sets with CPython's hash and set order — R1.2, R1.3
  _Depends 2.2_

## 3 · Functions

- [x] 3.1 (TDD) Compile lambdas and function values capturing by copy, and the prelude that takes them — R1.2
  _Depends 2.5_
- [x] 3.2 (TDD) Compile methods and the `inout`, `sink` and `var` conventions — R1.2, R3.5
  _Depends 2.3_
- [x] 3.3 (TDD) Compile generics, traits and `dyn` by monomorphization and vtables — R1.1, R1.2
  _Depends 3.2_
- [ ] 3.4 (Unit) Compile `Heap`, `math`, sized integers, conversions, the wrapping operations and the rest of the prelude — R1.2, R2.1
  _Depends 2.5_

## 4 · Running

- [ ] 4.1 (TDD) Run `parallel` on a thread per task, marking what each task captures shared — R4.1, R3.2, R3.3
  _Depends 3.1_
- [ ] 4.2 (Unit) Give `lotml run`, `test` and `build` the `--target c` option, with the Python target's test report — R1.4, R5.1
  _Depends 1.3_
- [ ] 4.3 (Unit) Call C libraries directly and link them, and refuse a Python import at the import — R5.3, R5.4
  _Depends 4.2_
