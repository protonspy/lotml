---
autonomy: auto
ci: wait
pr: per-group
merge: manual
status: approved
checksum: 6f494c68314de2896fdb7b77ccb3302d558a6224c910cfce1000ab65cda44f3e
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
- [x] 1.5 (Unit) Write the spec for the guide's tool: one MCP call taking the agent's state and answering with a location, a kind of change and at most one patch that `check` accepts, or nothing below the threshold, reachable from every harness `lotml init` registers
- [x] 2.1 (Unit) Decide in an ADR, from a pilot, the base model and size, the runtime it
      is served from
  _Depends 1.1, 1.3_
- [ ] 2.2 (Unit) Fine-tune the guide on the seeded failures and on the guidance records
      the licence registry permits — the seeded failures alone if it permits none — once
      the specs 1.2 to 1.4 wrote are built, and report it on the validation split
  _Depends 1.2, 1.3, 1.4, 2.1, 2.7, 2.8, 2.9_
- [ ] 2.3 (TDD) Train it further by reinforcement learning, the reward the location's
      overlap with the real fix and the patch passing `check` and the tests, and report
      the validation split before and after
  _Depends 2.2, 2.10_
- [ ] 2.4 (TDD) Calibrate the threshold on the validation split for a target precision,
      below which the guide says nothing
  _Depends 2.3, 2.11_
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
  _Status removed_
  _Reason review of the literature on 2026-10-08: re-added as 3.6, depending on the reordered chain_
- [x] 1.6 (Unit) Write the spec for MBPP's original as agent tasks, posed untyped the
      way specs/agent-humaneval/ poses HumanEval and as a delta to it, held out whole
      from training for the guide arms
- [x] 2.5 (Unit) Record in 2.1's ADR the runtime's first candidate — llama.cpp's
      `llama-server`, GGUF quantized for the CPU, answers held to the guide's JSON
      schema — and where training runs: the local GPU, or an on-demand RunPod GPU from a
      pinned Dockerfile with the Hugging Face stack, checkpoints and the model pushed to
      private Hugging Face repositories and resumed from there, the token a RunPod
      secret — with measured time, memory, cost and a budget per run, and record them in
      `docs/stack.md`
  _Depends 1.1, 1.3_
  _Priority 1_
  _Reason 45093e6 rewrapped 2.1 and cut it after 'the runtime it is served from', losing the runtime candidate and where training runs that ebebb6d added; specs/guide-tool/design.md and Done when still rely on them_
- [ ] 2.6 (Unit) Land the training pipeline from `feat/training-pipeline`: renumber its
      ADRs 0019 and 0020, which main gave to the phase 4 gate and the shared IR, to the
      next free numbers along with every citation of them in its code and specs, rebase
      on main and open its PR
  _Status removed_
  _Reason review of the literature on 2026-10-08: re-added as 2.7, which corrects the branch's spec and ADRs before landing it_
- [ ] 2.7 (Unit) Correct the training pipeline on `feat/training-pipeline`, then land
      it: renumber its ADRs 0019 and 0020, which main gave to the phase 4 gate and the
      shared IR, with every citation of them; credit an edit only when the file checks
      and every test block passes, hidden ones included; credit the first location the
      tool thresholds beside F1 over the named declarations, not F-beta 3; build the RL
      pool from records whose sampled rewards are not all equal, sampled from the
      rejection-sampled model; re-score the chosen checkpoint on the rest of the
      validation split; restate that SLMFix trained its fixer by RL alone, that
      2406.14867 favours distilling verified teacher code, and that FP16 matters when
      generation runs in a separate engine; record the survey report's 1.5B writer as a
      rejected option; rebase on main and open its PR
  _Reason review of the literature on 2026-10-08: the branch rewards recall the tool never reads and an edit the checker alone accepts, and its ADR numbers collide with main's; replaces 2.6_
- [ ] 2.8 (Unit) Add a syntax family to `lotml dev mutate` that writes the Python
      constructs lotml lacks, weighted by E0003's count in the phase 1 failures, keeping
      only mutants `check --fix` does not repair, and rebuild the seeded records
  _Reason review of the literature on 2026-10-08: E0003 is the largest first-error code no operator aims at (harness/results/seeded.md), and BIFI found synthetic training fails on the classes random perturbation rarely makes_
- [ ] 2.9 (Unit) Add harder seeded failures — several mutations in one program, programs
      that check but fail a test, and failures across files — each kept only when a
      hidden test fails and the known fix passes
  _Reason review of the literature on 2026-10-08: docs/wiki/pages/repair-training.md implication 3 had no task, and a saturated seeded split cannot steer RL_
- [ ] 2.10 (TDD) Before any RL, report on the seeded validation split the compiler's own
      top-1 and the share of single-declaration files, sample the fine-tuned guide eight
      times per train record, and count the records whose rewards are not all equal;
      below a pool size fixed before the run, strike 2.3 with that count as its reason
  _Depends 2.2_
  _Reason review of the literature on 2026-10-08: adr:0017 calls the seeded split saturated, possibly because most programs hold one declaration, and RL on records scored alike gives no gradient_
- [ ] 2.11 (TDD) Collect real failures on the validation problems from any model for the
      threshold 2.4 calibrates, and report no threshold when fewer than
      guide-evaluation's minimum are collected
  _Reason review of the literature on 2026-10-08: adr:0017 says a threshold set on seeded failures is no evidence of precision on real ones, and at 100% seeded top-1 it is degenerate_
- [ ] 3.4 (Unit) Size the guide arms 3.2 specifies by evaluation-rigor 1.2's minimum
      detectable effect at the cheap model's measured discordance — about 430 pairs at
      41% — never fewer than 168, compared by a paired test that holds with more than
      one run per task, and record it in that spec as a delta
  _Depends 3.2_
  _Status removed_
  _Reason review of the literature on 2026-10-08: its 'about 430 pairs at 41%' was an estimate; the exact test needs 336, so it is re-added as 3.6 with the computed sizes_
- [ ] 3.5 (Unit) Run the guide arms on MBPP's held-out tasks once the spec 1.6 wrote is
      built, commit the report, and record in the wiki whether the guide helps cheap
      agents, frontier ones, both or neither
  _Depends 1.5, 1.6, 3.1, 3.2, 3.6_
  _Reason review of the literature on 2026-10-08: 3.3 re-added unchanged, so it can also wait for the sizing task_
- [ ] 3.6 (Unit) Size the guide arms 3.2 specifies by evaluation-rigor 1.2's minimum
      detectable effect at the cheap model's measured discordance — the exact test needs
      336 pairs at 41% and 408 at 50% for 10 points — never fewer than 168, compared by
      a paired test that holds with more than one run per task, and record it in that
      spec as a delta
  _Depends 3.2_
  _Reason review of the literature on 2026-10-08: 168 pairs detect 10 points only at 20% discordant pairs; phase 1 measured 41% and 51.5% for the cheap models the guide is meant to help (docs/wiki/pages/evaluation-harness.md)_

## Done when

- The guide's offline report on held-out real failures is committed.
- The guide arms' report is committed: a cheap and a frontier model, two harnesses, with and
  without the guide, every task it made worse listed.
- `docs/wiki/pages/compiler-embedded-model.md` records the measured answer to its open question;
  an ADR fixes the model, the runtime and the compute; `scc validate` reports no findings.
