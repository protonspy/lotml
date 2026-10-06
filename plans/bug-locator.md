---
autonomy: auto
ci: wait
---

# Bug locator

Train a compiler-embedded model small enough to run on the CPU inside `lotml check` that points at
the lines most likely to be wrong in code that already checks — the errors no rule can state — as
warnings shown only above a calibrated confidence, and measure whether they help an agent.

## Why

lotml's compiler already reports, deterministically, every error a rule can state: syntax, types,
mutability, the neighbours' habits with their fixes. The study behind the compiler-embedded model
(plans/compiler-embedded-model.md, docs/wiki/pages/compiler-embedded-model.md) concludes that a
model earns a place only for what no rule expresses, behind a switch, and that its precision is
reached by suppression: AutoCommenter plateaued at 54% useful until non-actionable warnings were
cut. It also shows the size is within reach on a CPU — DeepBugs' 200-unit network answers in under
20 ms per file, SynShine's RoBERTa-base-sized classifier in 0.88 s, JetBrains ships 100M parameters
quantized to INT4 at about 100 MB and 150 ms — and warns that seeded bugs flatter: DeepBugs' 89–95%
on seeded bugs became 68% of its top 150 warnings on real code. So the model is a line classifier
trained with supervision, not a generator trained by reinforcement, and it is scored on real bugs:
the agent harness's runs give them, code that checks and fails its hidden tests until a later
version passes, the lines that changed being the bug. Done when the locator's precision, warnings
per line and CPU latency on held-out real bugs are committed, the agent arm with its warnings has
been measured against `check` alone, and the wiki records whether it ships.

## Paths

- `harness/lotml_harness/locator/` — the bug records, the mutations, the training and the evaluation
- `harness/lotml_harness/agent/` — the arm the warnings are measured in
- `compiler/crates/` — the inference behind `lotml check`, once the model has earned it
- `docs/adr/` — the model, the runtime and where training runs
- `docs/wiki/pages/compiler-embedded-model.md` — where the result is recorded

## References

- `plans/agent-data.md` — the HumanEval tasks, the traces and the licence registry this draws on
- `plans/compiler-embedded-model.md` — the study this plan acts on
- `docs/wiki/pages/compiler-embedded-model.md` — DeepBugs, SynShine, AutoCommenter, Ray et al., JetBrains' local model, Lean Copilot, with their numbers
- `docs/wiki/pages/semantic-compiler.md` — the diagnostics the warnings join, and the under-100 ms check they must not slow
- adr:0014-deepagents-over-openrouter-for-the-agent-harness
- adr:0015-pose-humaneval-untyped

## Out of scope

- Repairing code: the locator points, it does not rewrite; `check --fix` stays the repair for what
  a rule can state.
- Errors a rule can state: those become diagnostics with codes, as the study concludes, never
  model warnings.
- A model that writes or completes lotml, and reinforcement learning: a classifier trained with
  supervision fits the task, and the study found small models cannot author.
- Training on MultiPL-E or LiveCodeBench content, or on the phase 1 gate's answers to MultiPL-E's
  prompts (harness/results/NOTICE.md).

## Tasks

- [ ] 1.1 (Unit) Fix a split of every source by task id — train, validation, held-out evaluation — record it in the trace dataset's manifest, and keep held-out tasks out of every training set
- [ ] 1.2 (Unit) Write the spec for the bug records: from each run's traces, a version that checks and fails hidden tests paired with the next that passes, its changed lines labelled as the bug, through the licence registry
- [ ] 1.3 (Unit) Write the spec for the seeded bugs: lotml programs that check and pass their tests, mutated the ways agents get behaviour wrong — swapped arguments, an off-by-one bound, a wrong operator or variable — kept only when the mutant still checks and a test now fails
- [ ] 1.4 (Unit) Write the spec for the locator's evaluation: precision of its top warnings, warnings per hundred lines and recall on held-out real bugs, never seeded ones, and latency and memory on the CPU
- [ ] 2.1 (Unit) Decide in an ADR, from a pilot, the model family and size — a line classifier of the RoBERTa-base size or smaller — the CPU runtime it ships in, quantized, and where training runs, with measured time, memory and cost, and record them in `docs/stack.md`
  _Depends 1.1_
- [ ] 2.2 (Unit) Fine-tune the locator on the bug records and the seeded bugs, and report it on the validation split
  _Depends 1.2, 1.3, 1.4, 2.1_
- [ ] 2.3 (TDD) Calibrate the confidence threshold on the validation split for a target precision, the threshold below which nothing is shown
  _Depends 2.2_
- [ ] 3.1 (Unit) Report the locator on the held-out real bugs: precision, warnings per hundred lines, recall, and CPU latency per function and per file
  _Depends 2.3_
- [ ] 3.2 (Unit) Write the spec for the warnings arm: the agent harness showing the locator's warnings with `check`'s report, scored against `check` alone on pass@1, tokens and test calls
- [ ] 3.3 (Unit) Run the warnings arm and `check` alone on the held-out tasks, commit the report, and record in the wiki whether the locator ships in `lotml check`
  _Depends 3.1, 3.2_

## Done when

- The locator's report on held-out real bugs is committed: precision, warnings per hundred lines,
  recall, CPU latency and memory.
- The warnings arm's report is committed with `check` alone beside it.
- `docs/wiki/pages/compiler-embedded-model.md` records the measured answer, and whether the
  locator ships; an ADR fixes the model, the runtime and the compute; `scc validate` reports no
  findings.
