# Agent HumanEval — design

## What changes

Serves R1.1–R1.7, R2.1–R2.4.

A module `harness/lotml_harness/agent/humaneval.py`, three options on `python -m
lotml_harness.agent` — `--source humaneval` (the benchmark's tasks stay the default), `--sample N`,
`--seed S` (default 0) — and a report `harness/results/agent-humaneval.md`.

**Reading** (R1.1, R2.2). `tasks.sources.download` fetches
`https://raw.githubusercontent.com/openai/human-eval/<commit>/data/HumanEval.jsonl.gz` into
`harness/cache/human-eval/<commit>/`, with the commit and the file's SHA-256 pinned as constants
next to `MULTIPL_E` and `LIVECODEBENCH`. A digest that differs removes the file and stops.

**Translating** (R1.2, R1.6). Each JSONL record (`task_id`, `prompt`, `canonical_solution`,
`test`, `entry_point`) is joined into the text MultiPL-E's typed originals hold — the prompt, then
`### Canonical solution below ###` and the solution, then `### Unit tests below ###` and the
`check` — and handed to `tasks.build.translate` and `tasks.build.validate` as they are. The source
is named `humaneval-original`, apart from the task set's `humaneval`, so the two never mix in a
report or a dataset. What the translator refuses — a signature lotml cannot express, a `check`
statement that is not a comparison of literals — is refused here for the same reason and counted
(R1.7).

**From `Task` to `AgentTask`** (R1.3, R1.4). `AgentTask` today reads a directory; these tasks have
none on disk, and committing 164 directories would put HumanEval's text in the repository for no
gain. So `AgentTask` gets its files as data — `workspace`, `hidden` and `solution` as
`dict[str, str]`, the last empty here — `bench.load` filling them from a directory as now and
`humaneval.agent_task(task)` from a `Task`:

- workspace — `solution.lotml`: `task.prompt("b")` and `    return todo()`;
- hidden — one `test "hidden: case <i>":` per case, the call's arguments and the expected value
  rendered with `values.render`, compared with `==` for `eq`, `abs(got - want) <= 1e-6` for
  `approx`, `set(got) == set(want)` for `set`;
- kind `implement`, id `humaneval-<n>`.

`lay` writes the dictionaries instead of copying a tree, for both kinds of task.

**Prompt** (R1.5): "Implement `<name>` in `solution.lotml` as its docstring says. Keep its
signature." — fixed, so the arms differ only in their context.

**Sampling** (R2.1): `random.Random(seed)` shuffles the kept ids, sorted first, and takes N; the
draw depends on the seed and the file's contents only, so a run next month gets the same tasks.

**Rows and report** (R2.3, R2.4): rows gain `source`; the report groups pass@1 by it and, for
every model with both arms, gives the discordant pairs and McNemar's p with `variants.mcnemar`,
which the earlier experiments already use.

## Alternatives considered

- MultiPL-E's typed HumanEval, which the task set already holds: the same problems, but its
  licence forbids training on them, and the trace dataset is half of why these runs exist.
- Committing a directory per task under `harness/agent_bench/`: 164 copies of what one pinned,
  digested file already gives.
- Grading with the executor's `compare.matches` in Python instead of lotml `test` blocks: the
  hidden tests would leave the compiler's path the benchmark's tasks take, and the agent could not
  run their shape through `test` the way it runs its own.

## Risks

- Sample size: about 155 of the 164 should survive translation, below the 168 paired tasks a
  10-point difference needs at 80% power (docs/wiki/pages/evaluation-harness.md); the report says
  what difference the tasks it kept can detect. MBPP's original is the next source.
- Contamination: HumanEval is in most models' training data, Python's form of it at least; a lotml
  pass rate here is partly recall translated, which matters more for comparing languages than for
  comparing the two arms on the same tasks.
