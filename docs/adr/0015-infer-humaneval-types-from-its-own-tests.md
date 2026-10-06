---
status: accepted
---

# 0015 · Infer HumanEval's types from its own tests

## Context

The agent harness needs more tasks than its benchmark's eight, and the trace dataset needs tasks
whose licence allows training. The task set's HumanEval comes through MultiPL-E's typed Python
originals, whose licence forbids using its contents as training data (harness/results/NOTICE.md);
the original release, openai/human-eval, is MIT. lotml needs every parameter and return typed, and
measured on that release only 30 of its 164 entry points are fully annotated
(`def get_positive(l):` is typical). The types the task set reads were written by MultiPL-E, so they
are among the contents its licence restricts.

## Decision

The 134 functions the release leaves unannotated are typed by inference from the release's own
data: each parameter from the literal arguments its `check` passes, the return from the canonical
solution's results on them and the assertions' expected values, joined into one lotml type or
refused. A declared annotation is kept. MultiPL-E's files are not read for this, and the inferred
signatures are produced, listed in a report and verified by running the canonical solution on the
translated cases under them. The resulting tasks are a source of their own, `humaneval-original`.

## Consequences

- The tasks and the dataset made from them carry MIT's notice only, and nothing of MultiPL-E.
- An inferred type can be narrower than the docstring means — `[int]` where any number was meant —
  and still pass the canonical solution; the report lists every inferred signature so they are
  reviewed, and a task's type is corrected by adding the annotation the release lacks, recorded in
  the harness rather than in the data.
- The kept count is what the inference and the translator allow, measured rather than assumed;
  MBPP's original (CC BY 4.0) has the same gap and would take the same treatment.
- Tasks typed this way differ from MultiPL-E's in places, so `humaneval-original` results are not
  comparable task by task with the task set's `humaneval`.
