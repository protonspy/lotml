# Guide records — tasks

## 1 · The diff command

- [x] 1.1 (Unit) Find the declarations a change touches, innermost first, with their symbols, kinds and spans in both versions — R1.1
- [x] 1.2 (TDD) Find the one edit that reproduces a change, an arm before a body before a definition, then an added or removed declaration, then whole lines, reported only when it reproduces the second version byte for byte — R1.2, R1.3, R1.4
  _Depends 1.1_
- [x] 1.3 (Unit) Print both as `lotml dev diff <before> <after> --json`, the `dev` group hidden from help — R1.1, R1.2
  _Depends 1.2_

## 2 · The builder

- [x] 2.1 (Unit) Build a record's target from the diff command and its state through the guide tool's renderer, the kind named after the edit — R2.1, R2.2, R2.3
  _Depends 1.3_
- [x] 2.2 (Unit) Write each record with and without its task, marked with problem, split, origin, source, model and compiler version, train and validation apart, a held-out record stopping the build — R2.4, R2.5
  _Depends 2.1_
- [x] 2.3 (Unit) Drop and count a record over the context budget, then write the records to the cache and the committed report with every count by reason — R2.6, R2.7
  _Depends 2.2_
- [x] 2.4 (Unit) Keep each record's state beside its messages — R2.8
  _Depends 2.1_
  _Reason the training pipeline's reward judges an answer against the raw file (specs/training-pipeline/ 1.2)_
