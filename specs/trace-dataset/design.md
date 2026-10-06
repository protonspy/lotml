# Trace dataset — design

## What changes

Serves R1.1–R1.7, R2.1–R2.3, R3.1–R3.8.

A registry `harness/lotml_harness/agent/licences.toml`, a module `harness/lotml_harness/agent/
dataset.py` run as `python -m lotml_harness.agent.dataset`, two additions to the trace `run.py`
writes, and a scrub before anything is written under `harness/results/`. Output:
`harness/cache/dataset/<date>/` — `trajectories.jsonl`, `repairs.jsonl`, `NOTICE` and a copy of
the manifest — git-ignored since HumanEval's prompts are inside; the manifest
`harness/results/dataset.md`, committed.

**The registry** (R1.1, R1.4, R1.5). One TOML table per source, per model, and per provider under
its model, read with `tomllib`:

```toml
[sources.bench]
licence = "MIT"
training = "permitted"
evidence = "LICENSE"
checked = 2026-10-06
notice = "Copyright (c) the lotml authors"

[sources.humaneval-original]
licence = "MIT"
training = "permitted"
evidence = "https://github.com/openai/human-eval/blob/<40-character commit>/LICENSE"
checked = 2026-10-06
notice = "Copyright (c) OpenAI"

[sources.multipl-e-humaneval]   # the task set's typed translation, never an agent source here
licence = "BSD-3-Clause with a machine-learning restriction"
training = "forbidden"
evidence = "harness/results/NOTICE.md"
checked = 2026-10-06
notice = ""

[models."z-ai/glm-5.3-flash"]
terms = ""
training = "unknown"
evidence = ""
checked = ""
notice = ""

[models."z-ai/glm-5.3-flash".providers.Parasail]
training = "unknown"
evidence = ""
checked = ""
notice = ""
```

Default deny. An entry lets a run through only when `training` is exactly `permitted`, `evidence`
is an existing repository path or an `https://` URL, and `checked` and `notice` are set;
`notice` may be empty only for a forbidden entry. Anything else — `"Permitted"`, `true`, a missing
field, an unknown field — denies that entry; a registry that does not parse, or holds an unknown
table or a duplicate, stops the export with the reason (R1.5). A test fails if any `permitted`
entry lacks a field. The forbidden MultiPL-E entry is named so it cannot be read for the permitted
original, and no source of the agent harness is ever called `humaneval` alone.

The model's entry and its providers' start `unknown`: whether Z.ai's terms and those of each
provider OpenRouter routes it to allow training on the outputs is unverified, and the export stays
shut until someone records them (plans/agent-data.md, task 1.1). A grant given in conversation is
not evidence; a link to it in writing is. A row whose `providers` is empty is denied (R1.3): it
cannot show who served it.

**Identity** (R1.6). The exporter reads model, arm, task and providers from the `row` in each
trace, rejects a trace whose directory names differ from the row's, derives the source from the
task id (`humaneval-<n>` is `humaneval-original`, a benchmark directory name is `bench`) and
rejects a row whose `source` says otherwise or is missing. Symbolic links among the traces are
skipped. Output paths are built from constants and the date only.

**What the trace must carry** (R2.1, R2.2). The `Meter` snapshots the workspace's `.lotml` files
at each `check`'s start and end — `is_symlink` skipped, each file and the snapshot capped in bytes —
and keeps both with the parsed report and the call's arguments, so the trace gains
`checks: [{"paths": [...], "before": {...}, "after": {...}, "report": {...} | null,
"status": "success" | "error"}]` in call order. `lotml --version` is read once per run into the
trace and the row.

**Trajectories** (R3.1, R3.5). One record per passing run, in the OpenAI chat shape most trainers
read — `{"messages": [...], "tools": [...], "meta": {...}}` — the system prompt and memory as the
run had them, `tool_calls` on assistant turns, `tool` turns with their results. `meta` holds task,
source, model, arm, outcome, compiler version.

**Repairs** (R3.2–R3.4, R3.6). Walking a trace's `checks` in order, a check counts only when its
status is `success`, its report parsed as JSON and `before` equals `after`. A file it judged —
named in `paths`, or every file when `paths` is empty — with an error in the report opens a pending
repair; the next counting check that judged that file and reports no error in it closes it, with
the file at both points, the diagnostics, the task's prompt, and the changed line numbers from a
`difflib` diff of the two. A file still failing at the end of the run gives nothing. Records are
keyed by task and the SHA-256 of before, diagnostics and after; a key seen before is skipped.

The record's shape follows the study behind the compiler-embedded model
(docs/wiki/pages/compiler-embedded-model.md, plans/compiler-embedded-model.md): the (broken code,
compiler messages, fix) triple of HDLdebugger and DrRepair, whose repair rate fell from 62.5% to
34.0% on real errors without the message; the prompt, which SLMFix's fixer reads with the program
and the validator's output; the changed lines, because DrRepair found the line the compiler
reports is often not the one to fix; and deduplication per task, since FLAME's per-workbook
deduplication was worth 8 points over a global one. The agent's refused code is the real error
Break-It-Fix-It found decisive (90.5% against 62.7% for random corruption alone).

**Scrubbing** (R2.3, R3.7). Secrets: the values of every environment variable whose name ends in
`_KEY`, `_TOKEN` or `_SECRET`, compared exactly, and the shapes `sk-[A-Za-z0-9_-]{20,}`,
`ghp_[A-Za-z0-9]{30,}`, `hf_[A-Za-z0-9]{30,}`, `AKIA[0-9A-Z]{16}`, `Bearer [A-Za-z0-9._~+/-]{20,}`
and three-part JWTs. In a dataset record a secret drops the record; in a row or report bound for
`harness/results/` it is replaced with `<redacted>` before writing, the `error` field included.
Hidden tests: a record holding any `assert` line of the task's hidden blocks, compared after
collapsing whitespace, is dropped — the agent never sees those lines, so one in its record means a
leak. A docstring's example is a `>>>` line and never an `assert` line, so it does not trip it.

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
- Contamination: the dataset holds solutions to the benchmark's and HumanEval's tasks; the manifest
  lists the tasks it holds, so a model trained on it is never scored on them.
