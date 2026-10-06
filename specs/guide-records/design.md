# Guide records — design

## What changes

Serves R1.1–R1.4, R2.1–R2.7.

Two pieces: `lotml dev diff`, in the compiler, which says where a change falls and which edit
makes it; and `harness/lotml_harness/guide/records.py`, run as `python -m
lotml_harness.guide.records`, which builds the records from the trace dataset's export and the
seeded failures. It builds on three other specs, which come first: the export
(specs/trace-dataset/), the seeded failures (specs/seeded-failures/), and `lotml guide render`
(specs/guide-tool/, its task 2.2). Output: `harness/cache/guide/records/<date>/train.jsonl` and `validation.jsonl`,
git-ignored like the dataset they come from; the report `harness/results/guide-records.md`,
committed.

**Where and which edit belong to the compiler** (R1.1–R1.4). The spans of declarations and of
`match` arms, and the edit functions the MCP tools apply — `edit::replace` with its definition,
body and arm parts, `add`, `remove`, `search_replace` — live in `lotml-ide`. A function there,
`diff(before, after)`, walks `Workspace::outline` for both versions, takes the declarations of the
first whose spans hold a changed line (from a line diff of the two texts), innermost first — and for
a declaration only the second has, the one of the first it follows, or the file's start, since a
location must exist in the failing file the guide is shown — and then tries
edits from the smallest up: each arm of the innermost function whose span holds every changed line,
its body, its definition; a declaration only the second version has, added after the one before
it; one only the first has, removed; last, an `edit` whose search is the changed lines of the first
version, widened a line at a time until it occurs once, and whose replacement is the second's. An
edit is reported only if applying it gives the second text exactly (R1.3), so every patch the guide
learns is one the tools can apply as written. `lotml dev diff <before> <after> --path <name>
--json` prints, with `<name>` the file's path in the project, written into every edit's arguments
since `edit` needs it and the others need it when two files declare a symbol:

```json
{"declarations": [{"symbol": "median", "kind": "function", "lines": [3, 14]}],
 "edit": {"tool": "replace", "arguments": {"symbol": "median", "part": "body", "text": "…", "path": "stats.lotml"}}}
```

A start-of-file anchor has `symbol: null`; the guide tool keeps such a location, since it names a
place rather than a declaration (specs/guide-tool/).

`dev` is a hidden subcommand group: it serves the harness, not users, and stays out of `--help`.
Symbols are named as `show` and `replace` take them — `area`, `Counter.get`; a test block as
`test "name"`, which no symbol-addressed tool edits, so a change inside one is an `edit`.

**The builder** (R2.1–R2.7) reads `train/repairs.jsonl` and `validation/repairs.jsonl` from a
trace-dataset export and the seeded failures, which are written in the same repair shape with
`origin: seeded` (specs/seeded-failures/). For each, `lotml dev diff` on the file at both points
gives the target: `locations`, the changed declarations as path, symbol and lines in the failing
version, at most three; `kind`, the edit's tool and part (R2.2); `edit`, or null. The state is the
task, the failing file, and the diagnostics or the failing block, rendered by `lotml guide render`
(specs/guide-tool/) — the same code the tool runs at inference. A prompt built by a Python template
here and a Rust one there is train/serve skew that no test of either side catches; one renderer
removes it.

```json
{"messages": [{"role": "system", "content": "…"}, {"role": "user", "content": "…"},
              {"role": "assistant", "content": "{\"locations\": […], \"kind\": \"body\", \"edit\": {…}}"}],
 "meta": {"problem": "humaneval/12", "split": "train", "origin": "real", "source": "humaneval-original",
          "model": "z-ai/glm-5.3-flash", "compiler": "…", "renderer": 1, "task": true}}
```

A record with a task is written twice, with and without it (R2.4), since an agent may call the tool
without saying what it is doing; a HumanEval docstring or a failing block still carries the intent.
The context budget (R2.6) is the rule the guide's server applies at inference — the rendered
messages plus the answer's `max_tokens` within the model's context — counted here with the base
model's own tokenizer, which the ADR of plans/harness-guide.md 2.1 names with the context and the
answer budget. Records are built before the guide exists, so the builder takes the three as
arguments and refuses to run without them; at inference the server counts with the same tokenizer
and refuses the same overflow (specs/guide-tool/). Every record carries the renderer version it
was rendered with, and the guide's configuration takes its `renderer` from the records, so a
changed renderer cannot meet an old guide unnoticed. A held-out record cannot arrive — the export
writes none — and one that did would stop the build rather than be skipped (R2.5).

## Alternatives considered

- Computing the location from the diff in Python: the harness has no lotml parser, and arm spans
  and the edit functions would be written a second time, diverging from what the tools apply.
- The diagnostic's location as the target: what DrRepair found wrong, and what the evaluation's
  baseline already is (specs/guide-evaluation/); the guide is worth training only to beat it.
- A free-text explanation as the target: the study found unchecked explanations right about half
  the time and better-rated ones no faster to fix; a location and an edit can be checked.

## Risks

- `lines` and `several` may dominate if agents rewrite whole functions; the report counts kinds,
  and the location is still a target when the edit is not.
- Agent repairs carry the agent's style, and seeded ones the mutator's; the evaluation is on real
  failures only, so a guide that learned the mutator's tells is caught there.
