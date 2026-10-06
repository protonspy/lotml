# Agent HumanEval — design

## What changes

Serves R1.1–R1.9, R2.1–R2.4.

A module `harness/lotml_harness/agent/humaneval.py`, three options on `python -m
lotml_harness.agent` — `--source humaneval` (the benchmark's tasks stay the default), and with it
`--sample N` and `--seed S` (default 0); `--task humaneval-<n>` names one — and the report
`harness/results/agent-humaneval.md` of what was read, typed, kept and refused. Pass rates per
source and the arms' comparison go to `harness/results/agent.md`, which `report.markdown` already
writes.

**Reading** (R1.1, R2.2). `tasks.sources.download` fetches
`https://raw.githubusercontent.com/openai/human-eval/<commit>/data/HumanEval.jsonl.gz` into
`harness/cache/human-eval/<commit>/`, with the full 40-character commit and the file's SHA-256
pinned as constants next to `MULTIPL_E` and `LIVECODEBENCH`. `download` today returns a cached
file unchecked and moves a fresh one into place before anything could check it, so it gains an
optional digest: the `.part` file's bytes are checked before the rename, a cached file is checked
on every read, and one that differs is removed and stops the run. Decompression is bounded in bytes.

**Typing** (R1.2, R1.3; adr:0015-infer-humaneval-types-from-its-own-tests). Measured on the
release: 30 of 164 entry points are fully annotated, the rest not at all or in part
(`def get_positive(l):`). For each unannotated parameter, the literal arguments at its position
across the `check`'s assertions are mapped to lotml types — `bool` before `int`; `int` with `float`
widens to `f64`; a list, set or dict takes the join of its elements, empty ones contributing
nothing; a tuple, its positions; `None` among values makes the join optional — and the join must be
one type, else `no single type`. The return type joins the canonical solution's results on those
arguments, run in a child process as `tasks.canonical` runs solutions, and the assertions'
expected literals. A declared annotation is never overridden. The inferred types are written into
the `def` line, so the text handed on is the one the translator already reads.

**Translating** (R1.4, R1.8). Each JSONL record (`task_id`, `prompt` with its typed `def`,
`canonical_solution`, `test`, `entry_point`) is joined into the text MultiPL-E's typed originals
hold — the prompt, then `### Canonical solution below ###` and the solution, then `### Unit tests
below ###` and the `check` — and handed to `tasks.build.translate` and `tasks.build.validate` as
they are; `validate` runs the canonical solution on the translated cases under the given types, the
check that catches an inference too narrow. The source is named `humaneval-original`, apart from
the task set's `humaneval`, so the two never mix. Every refusal is counted by reason (R1.9). How
many tasks survive is measured by task 1.3 and recorded in the report and the plan, not assumed.

**From `Task` to `AgentTask`** (R1.5, R1.6). `AgentTask` today reads a directory and its
`hidden(file)` method reads `hidden/<file>`; these tasks have none on disk, and committing 164
directories would put HumanEval's text in the repository for no gain. So `AgentTask` carries its
files as data: `workspace_files`, `hidden_files` and `solution_files`, each `dict[str, str]`, and
`directory` becomes optional. `hidden(file)` stays, reading `hidden_files`; `bench.load` fills the
three from a directory as now, the only caller of `directory`. A HumanEval task has
`graded = ("solution.lotml",)` and an empty `solution_files`. The dataclass stays frozen and is
never hashed; holding dictionaries, it cannot be.

`humaneval.agent_task(task)` fills them from a `Task`:

- `solution.lotml`: `task.prompt("b")` and `    return todo()`;
- hidden — one `test "hidden: case <i>":` per case, the arguments and the expected value rendered
  with `values.render`, compared with `==` for `eq`; for `approx`, as `compare.matches` does,
  `abs(got - want) <= 1e-6` or within 1e-6 of the larger magnitude; `set(got) == set(want)` for
  `set`;
- kind `implement`, id `humaneval-<n>`.

`lay` writes the dictionaries instead of copying a tree, for both kinds of task, refusing a name
that is absolute, holds `..`, or resolves outside the target.

**Prompt** (R1.7): "Implement `<name>` in `solution.lotml` as its docstring says. Keep its
signature." — fixed, so the arms differ only in their context.

**Sampling** (R2.1): `random.Random(seed)` shuffles the kept ids, sorted first, and takes N; the
draw depends on the seed and the file's contents only, so a run next month gets the same tasks.
`--sample` without `--source humaneval` is an error.

**Rows and report** (R2.3, R2.4): every row, `error_row` included, gains `source`; a row written
before this change, without one, is reported as `bench` — every run before it was. The report
groups pass@1 by source and, for every model with both arms, pairs the arms on each task's attempt
0 and gives the discordant pairs and McNemar's p with `variants.mcnemar`, which the earlier
experiments already use.

## Alternatives considered

- MultiPL-E's typed HumanEval, which the task set already holds: the same problems, but its
  licence forbids training on them, and its types are part of what it restricts.
- Typing the 134 signatures by hand: as good where it is done carefully, but 134 judgements nobody
  re-checks, against an inference the canonical solution verifies on every case.
- Keeping only the 30 annotated functions: too few to compare the arms.
- Committing a directory per task under `harness/agent_bench/`: 164 copies of what one pinned,
  digested file already gives.
- Grading with `compare.matches` in Python instead of lotml `test` blocks: the hidden tests would
  leave the compiler's path the benchmark's tasks take.

## Risks

- Sample size: the kept count is unmeasured until task 1.3; if it falls below the 168 paired tasks
  a 10-point difference needs at 80% power (docs/wiki/pages/evaluation-harness.md), the report says
  what difference it can detect, and MBPP's original, typed the same way, is the next source.
- Inference too narrow or too wide: a list seen only as `[1, 2]` is typed `[int]` where the
  docstring meant numbers; `validate` catches a type the canonical solution breaks, not one that
  is merely narrower than intended. The report lists every inferred signature for review.
- Contamination: HumanEval is in most models' training data in Python; a lotml pass rate here is
  partly recall translated, which matters more for comparing languages than the two arms.
