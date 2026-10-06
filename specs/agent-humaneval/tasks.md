# Agent HumanEval — tasks

## 1 · HumanEval's tasks

- [x] 1.1 (TDD) Download the original `HumanEval.jsonl.gz` at a pinned commit, its SHA-256 checked before the rename and on every cached read, a differing file removed — R1.1, R2.2
- [x] 1.2 (Unit) Record a problem's cases by running its `check` against the canonical solution in a child process, seeded, arguments copied, duplicates dropped, at most 50 — R1.2
  _Depends 1.1_
- [x] 1.3 (TDD) Render recorded values as lotml literals by their Python types, refusing what has no lotml form, and compare floats within the tolerance — R1.5, R1.6
- [ ] 1.4 (TDD) Derive a witness signature from the recorded values and refuse a task whose hidden blocks do not check against it — R1.6
  _Depends 1.3_
- [ ] 1.5 (TDD) Give `AgentTask` its files as data, read from a directory as now, and lay them by writing them, refusing absolute, `..` and escaping names — R1.3, R1.7
- [ ] 1.6 (Unit) Build the agent task: `solution.lotml` with the untyped name, parameters, docstring and `todo()`, graded against it, the fixed prompt, no reference solution; and write the report of problems read, kept, refused by reason and cases recorded, with the digest and the MIT notice — R1.3, R1.4, R1.7, R1.8
  _Depends 1.2, 1.4, 1.5_

## 2 · Running and reporting

- [ ] 2.1 (TDD) Draw a seeded sample of the kept tasks, the same for every arm and model — R2.1
- [ ] 2.2 (Unit) Run HumanEval tasks from the command line with `--source humaneval`, `--sample`, `--seed` and `--task humaneval-<n>` — R2.1, R2.2
  _Depends 1.6, 2.1_
- [ ] 2.3 (Unit) Record every row's source, and report pass@1 per source, the arms' McNemar comparison on first attempts, and failures by signature apart from behaviour — R2.3, R2.4, R2.5
  _Depends 2.2_

## 3 · MBPP's tasks

- [ ] 3.1 (Unit) Download MBPP's original `mbpp.jsonl` at a pinned commit through the same digested download — R3.1
  _Depends 1.1_
- [ ] 3.2 (TDD) Find the one function an MBPP problem's tests call, refusing a problem whose tests call none or several, and record its outermost calls by running the setup and the asserts against the canonical code — R3.3, R3.4
  _Depends 1.2, 3.1_
- [ ] 3.3 (Unit) Build MBPP agent tasks as HumanEval's are built, the problem's text as the docstring, run them with `--source mbpp` and `--task mbpp-<n>`, and write their report with the CC BY 4.0 attribution — R3.2, R3.5, R3.6
  _Depends 1.6, 2.2, 3.2_
