---
autonomy: auto
ci: wait
pr: per-group
merge: manual
status: approved
checksum: 30217bc5b9271cc54df176736021c5fa2677425f794d2bb6d68a804028a260f5
---

# Harness guide

Train a small compiler-embedded model that guides any coding agent working in lotml — a cheap one or
a frontier one, in any harness — by telling it, when `check` refuses its code or a test fails, where
to change it and what kind of change it needs, served over MCP and silent when it is not confident,
and measure whether agents finish with it more often, or for fewer tokens.

## Why

The study behind the compiler-embedded model (plans/compiler-embedded-model.md,
docs/wiki/pages/compiler-embedded-model.md) left one question open: no study measures an agent
consuming a small model's output. Its evidence says where such a model can help and how it must
behave. Small models repair and locate rather than author (SLMFix's 0.5B fixer at 94.30% against
48.83% writing Ansible itself); the line a compiler reports is often not the one to fix (DrRepair);
guidance keyed to the error lifted RTLFixer to 98.5%; unchecked guidance is wrong about half the
time, so what is shown has to be checked or held back (PyFiXV, AutoCommenter, Lean Copilot,
Eiffel-tools). And the reader matters: in the phase 1 gate frontier models fixed nearly everything
from the diagnostics alone while 7–8B models did not recover, so a cheap agent can gain pass@1 and a
frontier one at most tokens and steps — the guide must never make it worse. MCP is what every
harness takes, and `lotml init` already registers lotml's server in Claude Code, Codex and Cursor.
Done when agents with and without the guide have been measured — a cheap and a frontier model, in
deepagents and in a second harness — on held-out tasks, and the wiki records whether it helps whom.

## Paths

- `harness/lotml_harness/guide/` — the guidance records, the mutations, the training and the evaluation
- `harness/lotml_harness/agent/` — the arms the guide is measured in
- `compiler/crates/lotml/` — the MCP tool through which harnesses reach the guide
- `docs/adr/` — the model, the runtime, where it runs and where training runs
- `docs/wiki/pages/compiler-embedded-model.md` — where the result is recorded

## References

- `plans/agent-data.md` — the HumanEval tasks, the traces and the licence registry this draws on
- `plans/compiler-embedded-model.md` — the study whose open question this answers
- `docs/wiki/pages/compiler-embedded-model.md` — SLMFix, DrRepair, Break-It-Fix-It, FLAME, DeepBugs, RTLFixer, PyFiXV, AutoCommenter, JetBrains' local model, with their numbers
- `docs/wiki/pages/training-prior.md` — RL with verifiable rewards, and what it needs from a new language
- `specs/agent-guide/` — `lotml init` and the MCP registration the guide is served through
- adr:0014-deepagents-over-openrouter-for-the-agent-harness
- adr:0015-pose-humaneval-untyped

- `specs/trace-dataset/` — the exporter and manifest the split and the guidance records are deltas to
- `specs/agent-humaneval/` — how a benchmark's original is posed untyped, which MBPP's original follows

## Out of scope

- Editing for the agent: the guide points and may propose a patch that checks; the agent decides
  and edits.
- Errors a rule can state: those become diagnostics with codes and `check --fix` fixes, never
  guidance.
- A model that writes lotml on its own: the study found small models cannot author.
- Running inside the compile step: the guide is a tool the agent or the harness calls, outside the
  under-100 ms check.
- Training on MultiPL-E or LiveCodeBench content, or on the phase 1 gate's answers to MultiPL-E's
  prompts (harness/results/NOTICE.md).

## Tasks

- [x] 1.1 (Unit) Fix a split by problem identity into train, validation and held-out,
      over every source and every derivative of one, MultiPL-E's HumanEval_N being the
      original's HumanEval/N and its MBPP problems the original's, with MBPP's original
      held out whole for the guide arms; record it as a delta to specs/trace-dataset/ so
      its manifest carries each record's split, and keep held-out problems out of every
      training set
- [x] 1.2 (Unit) Write the spec for the guidance records: from each run's traces, the
      state where the agent failed — code, diagnostics, the failing test's values, the
      task — paired with the edit that got it out, its symbol and lines as where, its
      diff as what, through the licence registry; the failing-test state is a delta to
      specs/trace-dataset/, whose repairs today open only on a check
- [x] 1.3 (Unit) Write the spec for the seeded failures: lotml programs that check and pass, mutated the ways agents break them, kept only when `check` refuses the mutant or a test then fails, with the known fix as the record
- [x] 1.4 (Unit) Write the spec for the guide's evaluation offline: top-1 and top-3
      location against the real fix, the share of proposed patches that check and pass,
      how often it stays silent, latency and memory, on held-out real failures, never
      seeded ones — from any model, since evaluating needs no permission to train, the
      phase 1 gate's refused answers to held-out problems included — with a minimum
      count of them fixed so top-1's 95% interval is no wider than ±10 points, and
      held-out runs on local models added until it is met
- [ ] 1.5 (Unit) Write the spec for the guide's tool: one MCP call taking the agent's state and answering with a location, a kind of change and at most one patch that `check` accepts, or nothing below the threshold, reachable from every harness `lotml init` registers
- [ ] 2.1 (Unit) Decide in an ADR, from a pilot, the base model and size, the runtime it
      is served from
  _Depends 1.1, 1.3_
- [ ] 2.2 (Unit) Fine-tune the guide on the seeded failures and on the guidance records
      the licence registry permits — the seeded failures alone if it permits none — once
      the specs 1.2 to 1.4 wrote are built, and report it on the validation split
  _Depends 1.2, 1.3, 1.4, 2.1_
- [ ] 2.3 (TDD) Train it further by reinforcement learning, the reward the location's overlap with the real fix and the patch passing `check` and the tests, and report the validation split before and after
  _Depends 2.2_
- [ ] 2.4 (TDD) Calibrate the threshold on the validation split for a target precision, below which the guide says nothing
  _Depends 2.3_
- [ ] 3.1 (Unit) Report the guide offline on the held-out real failures
  _Depends 2.4_
- [ ] 3.2 (Unit) Write the spec for the guide arms: the same agents with and without the
      guide's tool, a cheap model and a frontier one, in deepagents and in a second
      harness through MCP, on at least 168 paired MBPP tasks, scored on pass@1 by
      McNemar's test and on tokens, steps and cost over the tasks where the guide was
      called, with a budget per arm fixed before the run and any task the guide made
      worse listed
- [ ] 3.3 (Unit) Run the guide arms on MBPP's held-out tasks once the spec 1.6 wrote is
      built, commit the report, and record in the wiki whether the guide helps cheap
      agents, frontier ones, both or neither
  _Depends 1.5, 1.6, 3.1, 3.2_
- [ ] 1.6 (Unit) Write the spec for MBPP's original as agent tasks, posed untyped the
      way specs/agent-humaneval/ poses HumanEval and as a delta to it, held out whole
      from training for the guide arms

## Done when

- The guide's offline report on held-out real failures is committed.
- The guide arms' report is committed: a cheap and a frontier model, two harnesses, with and
  without the guide, every task it made worse listed.
- `docs/wiki/pages/compiler-embedded-model.md` records the measured answer to its open question;
  an ADR fixes the model, the runtime and the compute; `scc validate` reports no findings.
