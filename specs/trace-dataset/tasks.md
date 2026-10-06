# Trace dataset — tasks

## 1 · The registry

- [ ] 1.1 (TDD) Read the registry and decide, per run, whether it may be exported and, when not, why — R1.1, R1.2, R1.3
- [ ] 1.2 (Unit) Write the registry: this repository's tasks and the original HumanEval permitted with their licences and notices, MultiPL-E's forbidden, the model unknown — R1.1, R1.4

## 2 · The trace

- [ ] 2.1 (Unit) Keep the `.lotml` files and the report at every `check`, and the compiler's version, in the trace and the row — R2.1, R2.2

## 3 · Export

- [ ] 3.1 (Unit) Write a passing run's trajectory as one chat record with its tool calls, marked with its task, source, model, arm, outcome and compiler version — R3.1, R3.4
  _Depends 2.1_
- [ ] 3.2 (TDD) Extract repairs from a trace's checks, in any outcome, each identical repair once — R3.2, R3.3, R3.5
  _Depends 2.1_
- [ ] 3.3 (Unit) Drop and count a record holding a key-shaped string or a hidden test's text — R3.6
- [ ] 3.4 (Unit) Export from the command line to the git-ignored cache and write the committed manifest — R1.4, R3.7
  _Depends 1.1, 1.2, 3.1, 3.2, 3.3_
