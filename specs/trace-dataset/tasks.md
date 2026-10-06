# Trace dataset — tasks

## 1 · The registry

- [ ] 1.1 (TDD) Read the registry default-deny and decide, per run, whether it may be exported and, when not, why: exact `permitted`, evidence that resolves, every provider, a fatal error on a malformed registry — R1.1, R1.2, R1.3, R1.4, R1.5
- [ ] 1.2 (Unit) Write the registry: this repository's tasks and the original HumanEval permitted with their licences, evidence, dates and notices, MultiPL-E's forbidden, the model and its providers unknown — R1.1
- [ ] 1.3 (TDD) Take a run's identity from its trace's row, derive its source from the task id, and reject a trace whose path or source disagrees — R1.6

## 2 · The trace and what is committed

- [ ] 2.1 (Unit) Keep the `.lotml` files before and after every `check`, symbolic links skipped and sizes capped, with its normalised arguments, status and report; the compiler's version; and the main agent's system message and tool schemas, in the trace and the row — R2.1, R2.2, R2.4
- [ ] 2.2 (TDD) Redact secrets — environment values of eight characters or more and key shapes — from rows and reports before they are written under `harness/results/` — R2.3

## 3 · Export

- [ ] 3.1 (Unit) Write a passing run's trajectory as one chat record with its system message, tools and tool calls, marked with its task, source, model, arm, outcome and compiler version — R3.1, R3.5
  _Depends 2.1_
- [ ] 3.2 (TDD) Extract repairs from a trace's checks: only checks that succeeded, parsed and saw no change; only files they judged, directories included; in any outcome; marked like trajectories; each identical repair once per task — R3.2, R3.3, R3.4, R3.5, R3.6
  _Depends 2.1_
- [ ] 3.3 (TDD) Drop and count a record holding a secret, or an `assert` line of the task's hidden tests that its prompt does not also show — R3.7
  _Depends 2.2_
- [ ] 3.4 (Unit) Export from the command line to the git-ignored cache with the notices and the manifest beside it, and write the committed manifest with every count by reason — R1.7, R3.8
  _Depends 1.1, 1.2, 1.3, 3.1, 3.2, 3.3_
