---
autonomy: auto
ci: wait
---

# Agent HumanEval — requirements

## Purpose

The agent harness runs eight hand-written tasks, too few to compare its arms. HumanEval — 164
Python functions with a docstring and a `check` of assertions, written for Python and so close to
lotml's syntax — is MIT-licensed in its original release (openai/human-eval), which allows its use
as training data with the copyright notice, where the task set's HumanEval comes through MultiPL-E,
whose licence forbids it (harness/results/NOTICE.md). This makes the original HumanEval's tasks
agent tasks the harness runs, grades and reports like its own, and that the trace dataset may
export (specs/trace-dataset/, plans/agent-data.md).

## R1 · HumanEval's tasks

- **R1.1** The agent harness shall read HumanEval from the original release's `HumanEval.jsonl.gz` at a pinned 40-character commit, downloaded into the git-ignored cache, checking its recorded SHA-256 before the file is moved into place and again on every read.
- **R1.2** The agent harness shall translate each problem into a lotml signature, docstring and test cases with the task set's translator, refusing what it refuses for the reasons it gives.
- **R1.3** When an agent run names a HumanEval task, the agent harness shall build its workspace as one file, `solution.lotml`, holding the signature and docstring with `todo()` as the body.
- **R1.4** The agent harness shall turn each test case into a hidden `test` block comparing as the case's comparison says: exactly, within 1e-6, or as sets.
- **R1.5** The agent harness shall ask the agent to implement the function as its docstring says, keeping its signature, in the same words for every HumanEval task.
- **R1.6** Where a task comes from HumanEval, the agent harness shall keep it only if its canonical solution passes the translated cases under Python, and hold no lotml reference solution, in place of specs/agent-harness/ R1.3.
- **R1.7** The agent harness shall write a report of HumanEval's tasks read, kept and refused by reason, with the digest of the file it read and the MIT copyright notice.

## R2 · Running and reporting

- **R2.1** Where `--sample N` is given, the agent harness shall run N HumanEval tasks chosen by a seeded draw, the same N for every arm and every model.
- **R2.2** If the HumanEval file cannot be downloaded or its digest differs, then the agent harness shall say so and run nothing.
- **R2.3** The agent harness shall record each run's source, `bench` or `humaneval-original`, and report pass@1 per source.
- **R2.4** Where both arms have run a task, the agent harness shall report the paired comparison of the arms with McNemar's test over the tasks.

## Out of scope

- MBPP, LiveCodeBench and MultiPL-E's translations as agent tasks; MBPP's CC BY 4.0 original is
  the next source if more tasks are needed.
- Tasks of more than one file: HumanEval's are single functions.
