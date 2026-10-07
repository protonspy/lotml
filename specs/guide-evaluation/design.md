# Guide evaluation — design

## What changes

Serves R1.1–R1.4, R2.1–R2.8.

`harness/lotml_harness/guide/evaluate.py`, run as `python -m lotml_harness.guide.evaluate --config
<harness-guide.toml>`, with the guide served as the guide's tool expects (specs/guide-tool/). It
collects the failures, asks the guide about each, scores the answers against the real fix and the
compiler's baseline, and writes the report and rows. It builds on `lotml guide ask`
(specs/guide-tool/), `lotml dev diff` (specs/guide-records/), and the trace dataset's repair
extraction and problem split (specs/trace-dataset/), which come first; collecting more failures
also needs the agent harness to record checks and tests (specs/trace-dataset/, group 2) and the
HumanEval tasks (specs/agent-humaneval/).

**Collecting** (R1.1, R1.2). Two sources, read where they already are:

- agent traces under `harness/cache/agent/`: the trace dataset's extraction of check and test
  repairs is a pure function of a trace, and is reused without the licence registry — evaluating
  on a model's output is not training on it, so every model counts. The problem comes from the
  row's task through `split.py`.
- the phase 1 gate's rows, `harness/results/phase1/*.jsonl`: in a lotml conversation, a round
  whose outcome is not `pass` followed by a round that passes is a failure and its fix. The rows'
  `humaneval/<n>` and `mbpp/<n>` are problem ids already. Their tests are the hidden ones, so for a
  failing test the hidden block is the state's failing block. `harness/results/NOTICE.md` forbids
  training on these answers, not measuring with them; they never leave this module.

Every MBPP problem is held out, and the gate drew from both benchmarks, so its failures on MBPP
count alongside those on HumanEval's held-out quarter. Failures are keyed by problem and the
SHA-256 of the failing file, so the same mistake twice counts once.

**The minimum** (R1.4). 97 is where the normal approximation's 95% half-width at a rate of one half
falls to 10 points; Wilson's interval, which the report prints, is narrower there. Short of it the
report says how many more failures are needed and gives no verdict; plans/harness-guide.md 1.4
adds held-out runs on cheap models through the agent harness until it is met — models whose
mistakes are frequent, like the 7–8B ones the phase 1 gate saw refused 91 and 143 times in 200.

**The guard** (R1.3). The configuration names the records the guide was trained from
(specs/guide-records/). Before asking anything the evaluation reads every one, derives its problem
and split again through `split.py` rather than trusting `meta`, and stops — failing closed — when
`records` is absent, empty or unreadable, when a record has no problem or split, or when one is
held out. The records' SHA-256 goes in the report, so the number is tied to what was checked. A
guide trained on the wrong split is the failure that makes every other number here worthless, and
it is cheap to rule out.

**Asking** (R2.1, R2.8). Each failure's file goes into a fresh scratch directory, with its failing
block appended when the failure is a hidden test's, written by the agent harness's `lay` — which
refuses absolute names, `..`, reserved device names and any suffix but `.lotml` — and `lotml guide
ask --root <scratch> --task <task> -- <file>` runs there, the task given when one is known: the same
check, test, render, model call and gating the MCP tool runs, so what is scored is what an agent
would be shown. Every `lotml` call here, `dev diff` included, goes through one helper:
`execute.child_environment()`, `execute.limit_memory` — a job object on Windows, which ends the
call's children with it — and on POSIX a new session killed as a group, at a deadline above the
tool's own 75 s. Agent files come from traces of any model, so nothing about them is trusted.
Latency is the call's wall time; peak memory is the guide server's resident size, read from its
process before and after the run.

**Scoring** (R2.2–R2.4). `lotml dev diff` on the failing and fixed files gives the declarations the
fix changed. A location is right when its symbol is one of them; top-3 counts the first three
locations the guide ranks. The baseline is computed from the same state: the declarations holding
the first three diagnostics, deduplicated in order, or for a failing test the first function its
block calls. A guide that does not beat that baseline adds a model to say what the compiler said.
Edits are counted over those the guide proposed — an answer with an edit shown, or one whose
`withheld` names why it was not — so the report gives shown over proposed and the withheld by
reason; shown over shown would be 100% by construction.

```json
{"problem": "mbpp/412", "origin": "phase1", "model": "qwen2.5-coder-7b", "kind": "check",
 "truth": ["remove_odd"], "guide": {"locations": ["remove_odd"], "kind": "body", "edit": true,
 "silent": false, "confidence": 0.83}, "baseline": ["remove_odd"], "top1": true, "top3": true,
 "baseline_top1": true, "seconds": 0.41}
```

Rows hold symbols, booleans, numbers and the tool's fixed reason codes — no code and no free text
from the model or the server; a `lotml guide ask` that overran the evaluation's deadline is silent
for `deadline`, and one that crashed or printed no JSON for `unread`, the harness's own code — and pass the agent harness's scrub before writing, so committing
them commits no prompt and no secret; the report is built from rows alone.

## Alternatives considered

- Calling the model directly and re-implementing the gating here: two gatings would differ, and the
  number would describe neither.
- Exact line match as the location score: an agent edits a declaration through the symbol tools,
  and a fix's lines shift with its own length; line overlap is a score in the row for whoever
  wants it.
- Seeded failures held out from training: easier to get in number, and the evidence says they
  measure the generator.

## Risks

- The phase 1 conversations are single answers with feedback, not agent states; the report splits
  by origin, so a guide good on one and not the other shows.
- Few test failures: capable models fail `check` more than tests in lotml; the breakdown by kind
  says whether the meaning half of the guide was measured at all.
- The interval is optimistic: failures cluster by problem, model and repeated mistake, and the
  97 assumes them independent. Deduplicating by problem and file helps; the report also gives
  top-1 per problem. The held-out quarter is a hash bucket of about 41 problems, give or take
  five, fewer once agent-humaneval refuses some; validation, which calibrates the threshold, is
  about 25.
