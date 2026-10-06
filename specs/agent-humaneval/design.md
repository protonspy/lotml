# Agent HumanEval — design

## What changes

Serves R1.1–R1.8, R2.1–R2.5.

A module `harness/lotml_harness/agent/humaneval.py` with a child-process recorder, three options on
`python -m lotml_harness.agent` — `--source humaneval` (the benchmark's tasks stay the default), and
with it `--sample N` and `--seed S` (default 0); `--task humaneval-<n>` names one — and the report
`harness/results/agent-humaneval.md` of what was read, kept and refused. Pass rates per source and
the arms' comparison go to `harness/results/agent.md`, which `report.markdown` already writes.
adr:0015-pose-humaneval-untyped decides the shape; the task set's translator is not used, since it
needs the types the release lacks.

**Reading** (R1.1, R2.2). `tasks.sources.download` fetches
`https://raw.githubusercontent.com/openai/human-eval/<commit>/data/HumanEval.jsonl.gz` into
`harness/cache/human-eval/<commit>/`, with the full 40-character commit and the file's SHA-256
pinned as constants next to `MULTIPL_E` and `LIVECODEBENCH`. `download` today returns a cached
file unchecked and moves a fresh one into place before anything could check it, so it gains an
optional digest: the `.part` file's bytes are checked before the rename, a cached file is checked
on every read, and one that differs is removed and stops the run. Decompression is bounded in bytes.

**Cases** (R1.2). A child process, as `tasks.canonical` runs solutions — memory-limited, with a
timeout, since this is code from a dataset — executes the prompt and the canonical solution, then
the record's `test` with `random.seed(0)`, calling `check(recorder)`. The recorder calls the
canonical function, deep-copies the arguments before the call (some solutions change them), and
appends `(arguments, result)`. Every assertion in `check`, loops and random inputs included, thus
becomes a case whose expected value is the canonical solution's own — consistent by construction,
since the canonical solution passes its `check`. Duplicates are dropped and the first 50 kept.

**Literals** (R1.5, R1.6). A value becomes a lotml literal by its Python type: `bool` as `True` and
`False`, `int`, `float` by `repr` (`7.5`, `1.0`), `str` quoted as `values.render` quotes, `None` as
`None`, lists, tuples, sets and dicts element by element. A list mixing `int` and `float` writes
its integers as floats, since lotml has no implicit conversion and Python's list was numbers. A
value with no lotml form — a list of mixed kinds, an object — refuses the task. An expected value
holding a float compares as `compare.matches` does, within 1e-6 relative or absolute; others with
`==`.

**Satisfiable tests** (R1.6). A hidden block only measures the agent if some signature accepts
its literals. A witness is derived from the recorded values — each parameter's and the result's
types joined as lotml would type the literals: `[int]`, `str?` where `None` occurs, `f64` where a
float occurs — and `lotml check` runs on the witness signature with `todo()` and the hidden blocks
appended. A task they do not check against is refused as `unsatisfiable`. The witness is never
shown to the agent nor stored as an answer: it proves the tests can be met, nothing more.

**From the record to `AgentTask`** (R1.3, R1.4, R1.7). `AgentTask` today reads a directory and
its `hidden(file)` method reads `hidden/<file>`; these tasks have none on disk, and committing 164
directories would put HumanEval's text in the repository for no gain. So `AgentTask` carries its
files as data: `workspace_files`, `hidden_files` and `solution_files`, each `dict[str, str]`, and
`directory` becomes optional. `hidden(file)` stays, reading `hidden_files`; `bench.load` fills the
three from a directory as now, the only caller of `directory`. The dataclass stays frozen and is
never hashed; holding dictionaries, it cannot be.

A HumanEval task has kind `implement`, id `humaneval-<n>`, `graded = ("solution.lotml",)`, empty
`solution_files`, and `solution.lotml`:

```
fn has_close_elements(numbers, threshold):
    """ Check if in given list of numbers, are any two numbers closer to each other than
    given threshold. ... """
    return todo()
```

It does not check — lotml needs the types — and that is the task. The prompt (R1.4): "Write
`<name>` in `solution.lotml`: give its parameters and its return their lotml types, and implement it
as its docstring says." — fixed, so the arms differ only in their context. `lay` writes the
dictionaries instead of copying a tree, for both kinds of task, refusing a name that is absolute,
holds `..`, or resolves outside the target.

**Sampling** (R2.1): `random.Random(seed)` shuffles the kept ids, sorted first, and takes N; the
draw depends on the seed and the file's contents only. `--sample` without `--source humaneval` is
an error.

**Rows and report** (R2.3–R2.5): every row, `error_row` included, gains `source`; a row written
before this change, without one, is reported as `bench` — every run before it was. The report
groups pass@1 by source; for every model with both arms it pairs the arms on each task's attempt 0
and gives the discordant pairs and McNemar's p with `variants.mcnemar`. A failed HumanEval run is
counted as `signature` when the graded file checks alone but not with the hidden blocks appended —
the agent's types refused the values — and as `behaviour` otherwise; the grader records which.

## Alternatives considered

- Inferring the types ourselves and posing typed signatures: measures less (the agent never types
  anything) and puts a decision of ours in every task; the witness keeps only its useful half,
  proving the tests can be met.
- MultiPL-E's typed HumanEval, which the task set already holds: its licence forbids training on
  it, and its types are part of what it restricts.
- Translating `check`'s assertions statically, as the task set's extractor does: it refuses loops
  and random inputs, and needs the types; running `check` against a recorder takes every case.
- Committing a directory per task under `harness/agent_bench/`: 164 copies of what one pinned,
  digested file already gives.

## Risks

- Stricter than `check`: a case compares with the canonical solution's exact result, where `check`
  may have accepted more (an order it did not test, a tolerance). The report counts cases per task;
  a task every run fails is reviewed for this before it is believed.
- Literal typing: a test passing `5` where the docstring meant a number becomes an `int` argument,
  and an agent that typed `f64` fails it as `signature`. That is how lotml behaves, and R2.5 keeps
  it apart from wrong behaviour.
- Contamination: HumanEval is in most models' training data in Python; a lotml pass rate here is
  partly recall translated, which matters more for comparing languages than the two arms.
