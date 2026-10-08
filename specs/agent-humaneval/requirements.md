---
autonomy: auto
ci: wait
branch: feat/guide-evaluation
delivery: merged
pr: 23
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
(specs/trace-dataset/, plans/agent-data.md). MBPP's original release (CC BY 4.0) is posed the same
way, as the tasks the harness guide's arms run on: held out whole from training, and enough of them
for the 168 paired tasks a 10-point difference needs (plans/harness-guide.md).

## R1 · HumanEval's tasks

- **R1.1** The agent harness shall read HumanEval from the original release's `HumanEval.jsonl.gz` at a pinned 40-character commit, downloaded into the git-ignored cache, checking its recorded SHA-256 before the file is moved into place and again on every read.
- **R1.2** The agent harness shall take a problem's test cases by running its original `check` against its canonical solution under Python, random sources seeded, recording every call's arguments and result, at most 50 per problem in the order made, in a child process with the harness's clean environment, an empty working directory, a memory cap, a timeout and its output capped.
- **R1.3** When an agent run names a HumanEval task, the agent harness shall build its workspace as one file, `solution.lotml`, holding the function's name and parameter names without types, its docstring, and `todo()` as the body, graded against that file.
- **R1.4** The agent harness shall ask the agent to give the function's parameters and return their lotml types and to implement it as its docstring says, in the same words for every HumanEval task.
- **R1.5** The agent harness shall turn each case into a hidden `test` block whose arguments and expected value are lotml literals of the types the values had under Python, comparing within the harness's tolerance of 1e-6 relative or absolute where the expected value holds a float, and exactly otherwise.
- **R1.6** If a problem's values have no lotml literal, its docstring cannot be written as a lotml string, or its hidden blocks check against no signature typed from those values, then the agent harness shall refuse the task and count it by reason.
- **R1.7** Where a task comes from HumanEval, the agent harness shall hold no lotml reference solution, its cases being the canonical solution's own results, in place of specs/agent-harness/ R1.3.
- **R1.8** The agent harness shall write `harness/results/agent-humaneval.md`: the problems read, kept and refused by reason, the cases recorded, the digest of the file read, and the MIT copyright notice.

## R2 · Running and reporting

- **R2.1** Where `--source humaneval` and `--sample N` are given, the agent harness shall run N HumanEval tasks chosen by a seeded draw, the same N for every arm and every model.
- **R2.2** If the HumanEval file cannot be downloaded or its digest differs, then the agent harness shall say so and run nothing.
- **R2.3** The agent harness shall record each run's source, `bench` or `humaneval-original`, error rows included, and report pass@1 per source in `harness/results/agent.md`.
- **R2.4** Where both arms have run a task's first attempt, the agent harness shall report in `harness/results/agent.md` the paired comparison of the arms with McNemar's test over those first attempts.
- **R2.5** The agent harness shall report, for HumanEval runs that failed, how many failed because the agent's signature did not accept the hidden tests' values, apart from those whose function was wrong.

## R3 · MBPP's tasks

- **R3.1** The agent harness shall read MBPP from the original release's `mbpp.jsonl` at a pinned 40-character commit, checking its recorded SHA-256 as R1.1 checks HumanEval's.
- **R3.2** The agent harness shall pose an MBPP problem as R1.3 poses a HumanEval one: the function its `test_list` calls, with the parameter names its canonical `code` gives it, the problem's `text` as its docstring, and `todo()` as its body.
- **R3.3** The agent harness shall take an MBPP problem's cases by running its `test_setup_code` and `test_list` against its canonical code in the child process R1.2 runs, recording the arguments and result of every outermost call to the tested function, and turn them into hidden blocks as R1.5 and R1.6 do.
- **R3.4** If an MBPP problem's tests call more than one function of its code, or none, then the agent harness shall refuse it and count it by reason.
- **R3.5** Where `--source mbpp` is given, the agent harness shall run MBPP tasks as R2.1 runs HumanEval's, with task ids `mbpp-<n>` and the source `mbpp-original`.
- **R3.6** The agent harness shall write `harness/results/agent-mbpp.md`: the problems read, kept and refused by reason, the cases recorded, the digest of the file read, and the CC BY 4.0 attribution.

## Out of scope

- LiveCodeBench and MultiPL-E's translations as agent tasks.
- Tasks of more than one file: HumanEval's are single functions.
