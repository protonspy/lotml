# Shared IR — tasks

## 1 · The crate

- [x] 1.1 (Unit) Create `lotml-ir` with the C backend's intermediate form moved into it, a span on every statement, built-ins as an enum, and symbols made by `lotml_ir::symbol` — R1.1, R1.2, R1.3
- [x] 1.2 (Unit) Move lowering into `lotml-ir`, one instance per set of type arguments with the refusal of generics that need instances without end, and calls into Python modules lowered to `CallPython` — R1.1, R1.3, R1.4, R2.1, R3.3
  _Depends 1.1_
- [ ] 1.3 (Unit) Split instantiation out of lowering into the `mono` pass, with the refusal of generics that need instances without end — R3.1, R3.3
  _Depends 1.2_
  _Status removed_
  _Reason the LLVM backend reads only the monomorphic IR lowering already makes; the split serves the Python backend first and moves to specs/python-on-ir/ (plan reorder, 823750a)_
- [x] 1.4 (Unit) Move counting, reuse and uniqueness hoisting into `lotml-ir` behind `lotml_ir::native` — R3.1, R3.2
  _Depends 1.2_
- [x] 1.5 (Unit) Print the IR as text and verify it after every pass in test builds — R1.5, R1.6
  _Depends 1.1_

## 2 · The C backend on it

- [x] 2.1 (Unit) Compile C from the counted IR, removing the lowering and passes from `lotml-c`, and assert that `lotml check` lowers nothing — R4.1, R2.2
  _Depends 1.4_
- [ ] 2.2 (Unit) Run the parity suite and the C benchmarks, against a baseline run on the same machine before the move, and record both unchanged — R4.2, R4.3
  _Depends 2.1_
