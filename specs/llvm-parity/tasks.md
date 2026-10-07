# LLVM parity — tasks

## 1 · Values

- [x] 1.1 (Unit) Assert the runtime's struct layouts against the emitter's with the same `clang` — R3.1
- [x] 1.2 (TDD) Compile strings, f-strings and `str()` through the runtime's CPython rendering — R1.2, R1.3
  _Depends 1.1_
- [x] 1.3 (TDD) Compile lists, tuples, ranges and comprehensions with their counts and copies, and the leak report in test builds — R1.2, R3.1, R3.3
  _Depends 1.2_
- [x] 1.4 (TDD) Compile records, sum types, `match`, optionals and results with reuse — R1.2, R1.3, R3.1
  _Depends 1.3_
- [x] 1.5 (Unit) Compile dicts and sets, and the panics for indexes, keys and the other broken invariants — R1.3, R2.1
  _Depends 1.3_

## 2 · Functions and the rest

- [x] 2.1 (Unit) Compile closures, methods, `inout`, `sink`, `var`, generics, traits and `dyn` — R1.1, R1.2
  _Depends 1.4_
- [x] 2.2 (Unit) Compile `Heap`, `math`, sized integers, conversions, wrapping operations and the rest of the prelude — R1.1, R1.2
  _Depends 2.1_
- [x] 2.3 (Unit) Run `parallel` through the runtime and call `c.<library>` functions directly, linking the library — R3.2, R4.1
  _Depends 2.1_
- [x] 2.4 (Unit) Run `test` blocks through the runtime's test runner on `lotml test --target llvm` — R1.4
  _Depends 2.1_
- [x] 2.5 (Unit) Write line tables at `-O0` and check, with `llvm-dwarfdump --debug-line`, that they name the `.lot` lines — R2.2
  _Depends 2.1_

## 3 · Measured

- [x] 3.1 (Unit) Run the parity suite on the Python and LLVM targets and record it — R1.1, R5.1
  _Depends 2.2, 2.3, 2.4_
- [x] 3.2 (Unit) Run the benchmarks on the LLVM target against hand-written C and record them beside the C target's last run — R5.2
  _Depends 3.1_
