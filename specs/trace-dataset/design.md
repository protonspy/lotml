# Trace dataset — design

## What changes

Serves R1.1–R1.4, R2.1–R2.2, R3.1–R3.7.

A registry `harness/lotml_harness/agent/licences.toml`, a module `harness/lotml_harness/agent/
dataset.py` run as `python -m lotml_harness.agent.dataset`, and two additions to the trace `run.py`
writes. Output: `harness/cache/dataset/<date>/trajectories.jsonl` and `repairs.jsonl`, git-ignored
since HumanEval's prompts are inside; the manifest `harness/results/dataset.md`, committed.

**The registry** (R1.1). One TOML table per source and per model, read with `tomllib`:

```toml
[sources.bench]
licence = "MIT"
training = "permitted"
evidence = "LICENSE"
notice = "Copyright (c) the lotml authors"

[sources.humaneval-original]
licence = "MIT"
training = "permitted"
evidence = "https://github.com/openai/human-eval/blob/<commit>/LICENSE"
notice = "Copyright (c) OpenAI"

[sources.humaneval]   # MultiPL-E's typed translation, the task set's
licence = "BSD-3-Clause with a machine-learning restriction"
training = "forbidden"
evidence = "harness/results/NOTICE.md"

[models."z-ai/glm-5.3-flash"]
terms = ""
training = "unknown"
evidence = ""
```

`training` is `permitted`, `forbidden` or `unknown`; only `permitted` with a non-empty `evidence`
lets a run through (R1.2, R1.3). The model's entry starts `unknown`: whether Z.ai's and the
serving provider's terms allow training on the outputs is unverified, and the exporter stays shut
for that model until someone records them (plans/agent-data.md, task 1.1). A grant given in
conversation is not evidence; a link to it in writing is.

**What the trace must carry** (R2.1, R2.2). The `Meter` already sees each tool's end; on `check`'s
it snapshots the workspace's `.lotml` files and keeps them with the report, so the trace gains
`checks: [{"files": {...}, "report": {...}}]` in call order. `lotml --version` is read once per run
into the trace and the row. Snapshots are of a scratch workspace of a few files, bounded by the
step limit.

**Trajectories** (R3.1, R3.4). One record per passing run, in the OpenAI chat shape most trainers
read — `{"messages": [...], "tools": [...], "meta": {...}}` — the system prompt and memory as the
run had them, `tool_calls` on assistant turns, `tool` turns with their results. `meta` holds task,
source, model, arm, outcome, compiler version.

**Repairs** (R3.2, R3.3, R3.5). Walking a trace's `checks` in order, a file whose report names an
error opens a pending repair; the next `check` in which that file has none closes it, with the
file at both points and the diagnostics in between. A file still failing at the end of the run
gives nothing. Records are keyed by the SHA-256 of before, diagnostics and after; a key seen
before is skipped.

**Scrubbing** (R3.6). A record matching `sk-[A-Za-z0-9_-]{20,}` (OpenRouter's `sk-or-…`, and
others' shapes) is dropped and counted; so is one containing a hidden test block's text, which can
only mean the hidden tests leaked into the agent's context.

## Alternatives considered

- A flag on each row instead of a registry: rows are written by runs, a licence is decided once
  per source, and a decision recorded in a thousand rows cannot be corrected in one place.
- Exporting every run and filtering at training time: the dataset would hold MultiPL-E's content
  that may not be trained on, one careless step away from a trainer.
- Repairs from consecutive edits rather than from `check` results: an edit that checks is only
  known to check when `check` says so; the compiler is the critic, as the wiki concludes.

## Risks

- Few repairs: a capable model writes code that checks the first time; the counts in the manifest
  say whether mutation (out of scope) is needed.
- Stale data: a record carries the compiler version that judged it, because a rule that changes
  makes old repairs wrong, the failure the wiki records for trained models in a moving language.
