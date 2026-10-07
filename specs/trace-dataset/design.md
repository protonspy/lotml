# Trace dataset — design

## What changes

Serves R1.1–R1.7, R2.1–R2.5, R3.1–R3.9, R4.1–R4.6.

A registry `harness/lotml_harness/agent/licences.toml`, a module `harness/lotml_harness/agent/
dataset.py` run as `python -m lotml_harness.agent.dataset`, the problem split in
`harness/lotml_harness/split.py`, two additions to the trace `run.py` writes, and a scrub before
anything is written under `harness/results/`. Output: `harness/cache/dataset/<date>/` —
`train/` and `validation/`, each with `trajectories.jsonl` and `repairs.jsonl`, then `NOTICE` and
a copy of the manifest — git-ignored since HumanEval's prompts are inside; the manifest
`harness/results/dataset.md`, committed.

**The registry** (R1.1, R1.4, R1.5). One TOML table per source, per model, and per provider under
its model, read with `tomllib`; model and provider names are quoted keys, since OpenRouter's
(`"Z.AI"`, `"Together AI"`) hold dots and spaces. Providers are added as rows report them; the one
below is illustrative:

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

[models."z-ai/glm-5.3-flash".providers."Parasail"]
training = "unknown"
evidence = ""
checked = ""
notice = ""
```

Default deny. An entry lets a run through only when `training` is exactly `permitted`, `evidence`
is an existing repository path or an `https://` URL, and `checked` and `notice` are set;
`notice` may be empty only for a forbidden entry. A value other than those — `"Permitted"`,
`true`, an empty or missing field — denies that entry; a registry that does not parse, or holds an
unknown table, an unknown field or a duplicate, stops the export with the reason (R1.5). A test fails if any `permitted`
entry lacks a field. The forbidden MultiPL-E entry is named so it cannot be read for the permitted
original, and no source of the agent harness is ever called `humaneval` alone.

The model's entry and its providers' start `unknown`: whether Z.ai's terms and those of each
provider OpenRouter routes it to allow training on the outputs is unverified, and the export stays
shut until someone records them (plans/agent-data.md, task 1.1). A grant given in conversation is
not evidence; a link to it in writing is. A row whose `providers` is empty is denied (R1.3): it
cannot show who served it. A response whose metadata names no provider is counted under `unnamed`,
which the registry holds no entry for, so one such call denies the whole run.

**Identity** (R1.6). The exporter reads model, arm, task and providers from the `row` in each
trace, rejects a trace whose directory names differ from the row's, derives the source from the
task id (`humaneval-<n>` is `humaneval-original`, `mbpp-<n>` is `mbpp-original`, a benchmark directory name is `bench`, and
any other spelling is `unknown` — MultiPL-E's `HumanEval_<n>_<name>` among them, though the split
maps it to its problem) and rejects a row whose `source` says otherwise or is missing. Symbolic links among the traces are
skipped. Output paths are built from constants and the date only.

**What the trace must carry** (R2.1, R2.2, R2.4). The `Meter` snapshots the workspace's `.lotml`
files at each `check`'s start and end — `is_symlink` skipped, each file and the snapshot capped in
bytes — and keeps both with the parsed report and the call's arguments, taken from the callback's
`inputs` as JSON rather than its stringified `input_str`, and normalised as the MCP wrapper
normalises them: the leading `/` the file tools show is stripped, so `paths` and snapshot keys are
both workspace-relative. The trace gains `checks: [{"paths": [...], "before": {...},
"after": {...}, "report": {...} | null, "status": "success" | "error"}]` in call order, and
`tests` in the same shape for every `test` call (R2.5), whose report is `lotml test --json`'s. A
`test` report can carry Python's load errors, which name host paths; the home directory and the
user's name, where it is a whole segment of a path, are replaced with `~` and `<user>` before the
trace is written, and the exporter does the same to every record it writes. A snapshot cut at its
cap marks the call `truncated`, which R3.4 treats as files changed, so a cut file never opens or
closes a repair. Of the model's invocation parameters only the tool schemas are kept.
`lotml --version` is read once per run into the trace and the row. The first call of the main
agent's model is caught in `on_chat_model_start`: its system message — deepagents' base prompt, the
harness's, and the memory it injects — and the tool schemas in its invocation parameters are kept
as `system` and `tools`; the state's messages never hold them.

**Trajectories** (R3.1, R3.5). One record per passing run, in the OpenAI chat shape most trainers
read — `{"messages": [...], "tools": [...], "meta": {...}}` — the kept system message first,
`tool_calls` on assistant turns, `tool` turns with their results. `meta` holds task, source, model,
arm, outcome, compiler version, problem and split (R4.5). A trajectory is the main agent's: a deepagents subagent's calls
never reach its state and appear only as the `task` tool's call and result.

**Repairs** (R3.2–R3.4, R3.6). Walking a trace's `checks` in order, a check counts only when its
status is `success`, its report parsed as JSON and `before` equals `after`. A file it judged —
named in `paths`, under a directory named there (`.` names all), or every file when `paths` is
empty, as the server's own selection reads them — with an error in the report opens a pending
repair; the next counting check that judged that file and reports no error in it closes it, with
the file at both points, the diagnostics, the task's prompt, and the changed line numbers from a
`difflib` diff of the two. A file still failing at the end of the run gives nothing. Records are
keyed by task and the SHA-256 of before, diagnostics and after; a key seen before is skipped. A
repair's `meta` is a trajectory's with `origin: real`, the field the seeded failures set to `seeded`
(specs/seeded-failures/), so the guide's records tell the two apart.

A test repair (R3.9) walks `tests` the same way, with the same counting rule (R3.4). `lotml test`
runs no block in files that do not compile — its report is then the compile errors — so a block
reported failing is itself the proof that the file checks, with no `check` call needed before it:
its state is a program that compiles and does the wrong thing — what the harness guide must locate when
no diagnostic points anywhere (plans/harness-guide.md). The next counting `test` reporting that
block passing closes it with the file at both points, the block's name and the values each side of
its comparison had, and the changed lines. A block the agent fixed by editing the block itself is
kept: an expected value the agent got wrong is a real failure too, and the changed lines say where
it was.

The record's shape follows the study behind the compiler-embedded model
(docs/wiki/pages/compiler-embedded-model.md, plans/compiler-embedded-model.md): the (broken code,
compiler messages, fix) triple HDLdebugger built and DrRepair trained on — DrRepair's model, given
no compiler message, looked the same on synthetic data and fell to 34.0% on the real test set; the
prompt, which SLMFix's fixer reads with the program
and the validator's output; the changed lines, because DrRepair found the line the compiler
reports is often not the one to fix; and deduplication per task, since FLAME's per-workbook
deduplication was worth 8 points over a global one. The agent's refused code is the real error
Break-It-Fix-It found decisive (90.5% against 62.7% for random corruption alone).

**The split** (R4.1–R4.6). `split.py` is one module because three consumers need the same answer:
this exporter, the seeded failures and the guide's evaluation (plans/harness-guide.md). It holds
two functions. `problem(id)` maps an id to its original problem by pattern — `humaneval-<n>` and
`HumanEval_<n>_<name>` to `humaneval/<n>`, `mbpp-<n>` and `mbpp_<n>_<name>` to `mbpp/<n>`, the
name of a task `bench.load` lists to `bench/<name>`; the task set's own `humaneval/<n>` and `mbpp/<n>`,
which the phase 1 gate's rows carry, are problem ids already — and raises on anything else (R4.4), so a new source
fails loudly instead of landing in train. `split(problem)` buckets HumanEval by the first eight
bytes of `sha256(SALT + problem)` read as an integer over 2^64: below 0.60 train, below 0.75
validation, otherwise held-out. The salt is a constant, `lotml-split-1`; changing it makes a new
split, so it never changes once a guide has trained on one. MBPP is held out whole, since it is
where the guide's arms run; the eight benchmark tasks are ours and too few to hold out.

The bucket depends on the problem alone, so the agent's run, MultiPL-E's translation in the phase
1 gate and a seeded mutant of the same HumanEval problem always land together — the leak a split by
row or by source would let through. A quarter held out is about 41 HumanEval problems; their
failures, from any model, are the guide's offline evaluation, whose own spec fixes how many it
needs. Held-out records are never written, so no trainer can read one; the evaluation reads the
traces itself.

**Scrubbing** (R2.3, R3.7). Secrets: the values of every environment variable whose name ends in
`_KEY`, `_TOKEN`, `_SECRET`, `_PASSWORD` or `_PASS`, compared exactly and only when eight characters or longer — an
empty or short value would match everything — and the shapes `sk-[A-Za-z0-9_-]{20,}`,
`ghp_[A-Za-z0-9]{30,}`, `hf_[A-Za-z0-9]{30,}`, `AKIA[0-9A-Z]{16}`, `Bearer [A-Za-z0-9._~+/-]{20,}`
and three-part JWTs. In a dataset record a secret drops the record; in a row or report bound for
`harness/results/` it is replaced with `<redacted>` before writing, the `error` field included.
Hidden tests: a record holding an `assert` line of the task's hidden blocks, compared after
collapsing whitespace and matched whole, a trailing comment allowed, is dropped — unless the
task's prompt pairs the same call with the same expected value, as a docstring's example does
(`>>> f(x)` and the value on the next line, or `f(x) == y`), or holds the whole assert. A call and
a value found apart in the prompt do not count: a short value such as `2` is in almost any
prompt. Many HumanEval cases are its docstring's examples, and an agent that copies one into its
own `test` block writes an identical line; dropping it would remove exactly the runs that test
themselves. Every drop is counted by reason in the manifest, so the bias it could introduce shows.

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
- Contamination: the dataset holds solutions to the benchmark's and HumanEval's tasks; it holds no
  held-out problem, and the manifest counts each split, so a model trained on it is never scored on
  a problem it saw.
