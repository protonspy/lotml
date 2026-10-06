# Agent HumanEval — tasks

## 1 · HumanEval's tasks

- [ ] 1.1 (Unit) Download the original `HumanEval.jsonl.gz` at a pinned commit, checked against its SHA-256, refusing a file that differs — R1.1, R2.2
- [ ] 1.2 (Unit) Translate its records with the task set's translator and validator as source `humaneval-original`, and write the report of kept and refused tasks with the MIT notice — R1.2, R1.6, R1.7
  _Depends 1.1_
- [ ] 1.3 (Unit) Give `AgentTask` its files as data, read from a directory as now, and lay them by writing them — R1.6
- [ ] 1.4 (TDD) Render a task's cases as hidden `test` blocks for each comparison — R1.4
- [ ] 1.5 (Unit) Build the agent task: `solution.lotml` with `todo()`, the fixed prompt, no reference solution; check on every kept task that its hidden blocks check against the signature and that `todo()` fails them — R1.3, R1.4, R1.5, R1.6
  _Depends 1.2, 1.3, 1.4_

## 2 · Running and reporting

- [ ] 2.1 (TDD) Draw a seeded sample of the kept tasks, the same for every arm and model — R2.1
- [ ] 2.2 (Unit) Run HumanEval tasks from the command line with `--source humaneval`, `--sample` and `--seed` — R2.1, R2.2
  _Depends 1.5, 2.1_
- [ ] 2.3 (Unit) Record each row's source and report pass@1 per source and the arms' McNemar comparison — R2.3, R2.4
  _Depends 2.2_
