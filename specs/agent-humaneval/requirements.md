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
whose licence forbids it (harness/results/NOTICE.md). Only 30 of the original's functions are fully
annotated, and the types MultiPL-E added are part of what its licence restricts. So every task is
posed without types: the agent writes the lotml signature from the docstring, as a developer
porting Python would (adr:0015-pose-humaneval-untyped). This makes all of HumanEval agent tasks the
harness runs, grades and reports like its own, and that the trace dataset may export
(specs/trace-dataset/, plans/agent-data.md).

## R1 · HumanEval's tasks

- **R1.1** The agent harness shall read HumanEval from the original release's `HumanEval.jsonl.gz` at a pinned 40-character commit, downloaded into the git-ignored cache, checking its recorded SHA-256 before the file is moved into place and again on every read.
- **R1.2** The agent harness shall take a problem's test cases by running its original `check` against its canonical solution under Python, random sources seeded, recording every call's arguments and result, at most 50 per problem in the order made.
- **R1.3** When an agent run names a HumanEval task, the agent harness shall build its workspace as one file, `solution.lotml`, holding the function's name and parameter names without types, its docstring, and `todo()` as the body, graded against that file.
- **R1.4** The agent harness shall ask the agent to give the function's parameters and return their lotml types and to implement it as its docstring says, in the same words for every HumanEval task.
- **R1.5** The agent harness shall turn each case into a hidden `test` block whose arguments and expected value are lotml literals of the types the values had under Python, comparing within the harness's tolerance of 1e-6 relative or absolute where the expected value holds a float, and exactly otherwise.
- **R1.6** If a problem's values have no lotml literal, or its hidden blocks check against no signature typed from those values, then the agent harness shall refuse the task and count it by reason.
- **R1.7** Where a task comes from HumanEval, the agent harness shall hold no lotml reference solution, its cases being the canonical solution's own results, in place of specs/agent-harness/ R1.3.
- **R1.8** The agent harness shall write `harness/results/agent-humaneval.md`: the problems read, kept and refused by reason, the cases recorded, the digest of the file read, and the MIT copyright notice.

## R2 · Running and reporting

- **R2.1** Where `--source humaneval` and `--sample N` are given, the agent harness shall run N HumanEval tasks chosen by a seeded draw, the same N for every arm and every model.
- **R2.2** If the HumanEval file cannot be downloaded or its digest differs, then the agent harness shall say so and run nothing.
- **R2.3** The agent harness shall record each run's source, `bench` or `humaneval-original`, error rows included, and report pass@1 per source in `harness/results/agent.md`.
- **R2.4** Where both arms have run a task's first attempt, the agent harness shall report in `harness/results/agent.md` the paired comparison of the arms with McNemar's test over those first attempts.
- **R2.5** The agent harness shall report, for HumanEval runs that failed, how many failed because the agent's signature did not accept the hidden tests' values, apart from those whose function was wrong.

## Out of scope

- MBPP, LiveCodeBench and MultiPL-E's translations as agent tasks; MBPP's CC BY 4.0 original is
  the next source if more tasks are needed, posed the same way.
- Tasks of more than one file: HumanEval's are single functions.
