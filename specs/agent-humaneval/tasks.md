# Agent HumanEval — tasks

## 1 · HumanEval's tasks

- [ ] 1.1 (TDD) Download the original `HumanEval.jsonl.gz` at a pinned commit, its SHA-256 checked before the rename and on every cached read, a differing file removed — R1.1, R2.2
- [ ] 1.2 (TDD) Infer an unannotated parameter's or return's type from the `check`'s literals and the canonical solution's results, refusing values with no single type — R1.2, R1.3
- [ ] 1.3 (Unit) Translate the records, typed, with the task set's translator and validator as source `humaneval-original`, and write the report: read, declared, inferred, kept, refused by reason, every inferred signature, the digest and the MIT notice — R1.4, R1.8, R1.9
  _Depends 1.1, 1.2_
- [ ] 1.4 (TDD) Give `AgentTask` its files as data, read from a directory as now, and lay them by writing them, refusing absolute, `..` and escaping names — R1.5, R1.8
- [ ] 1.5 (TDD) Render a task's cases as hidden `test` blocks for each comparison — R1.6
- [ ] 1.6 (Unit) Build the agent task: `solution.lotml` with `todo()`, graded against it, the fixed prompt, no reference solution; check on every kept task that its hidden blocks check against the signature and that `todo()` fails them — R1.5, R1.6, R1.7, R1.8
  _Depends 1.3, 1.4, 1.5_

## 2 · Running and reporting

- [ ] 2.1 (TDD) Draw a seeded sample of the kept tasks, the same for every arm and model — R2.1
- [ ] 2.2 (Unit) Run HumanEval tasks from the command line with `--source humaneval`, `--sample`, `--seed` and `--task humaneval-<n>` — R2.1, R2.2
  _Depends 1.6, 2.1_
- [ ] 2.3 (Unit) Record every row's source and report pass@1 per source and the arms' McNemar comparison on first attempts — R2.3, R2.4
  _Depends 2.2_
