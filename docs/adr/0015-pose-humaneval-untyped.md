---
status: accepted
---

# 0015 · Pose HumanEval untyped: the agent writes the types

## Context

The agent harness needs more tasks than its benchmark's eight, and the trace dataset needs tasks
whose licence allows training. The task set's HumanEval comes through MultiPL-E's typed Python
originals, whose licence forbids using its contents as training data (harness/results/NOTICE.md);
the original release, openai/human-eval, is MIT. lotml needs every parameter and return typed, and
measured on that release only 30 of its 164 entry points are fully annotated
(`def get_positive(l):` is typical). The types the task set reads were written by MultiPL-E, so they
are among the contents its licence restricts. Inferring the missing types ourselves was considered
first.

## Decision

Every HumanEval task is posed without types: `solution.lotml` holds the function's name, its
parameter names, its docstring and `todo()`, and the agent gives the types along with the body, as
a developer porting a Python function to lotml would. The hidden tests are the canonical solution's
own results, recorded by running each problem's original `check` against it, and written as lotml
literals of the types the values had under Python. A signature derived from those values is used
only to prove the hidden tests can be met; the agent never sees it. MultiPL-E's files are not read,
and the tasks are a source of their own, `humaneval-original`.

## Consequences

- All of HumanEval is posed, the 134 untyped functions included, and the tasks and any dataset
  made from them carry MIT's notice only.
- A task now measures typing as well as implementing: reading `>>> triangle_area(5, 3)` and
  choosing `int` parameters and an `f64` result. A run that fails because the agent's signature
  refused the tests' values is reported apart from one whose function was wrong.
- lotml's lack of implicit conversion shows: a test passing `5` where the docstring speaks of
  numbers refuses an agent that typed `f64`. That is lotml behaving as specified, not noise, and
  the separate count keeps it visible.
- Results are not comparable task by task with the task set's typed `humaneval`, nor with
  published HumanEval scores, whose functions arrive typed or untyped in Python.
