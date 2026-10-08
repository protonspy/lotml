---
autonomy: auto
ci: wait
pr: per-group
merge: manual
status: approved
checksum: ae39f1f99aa3dcf54d5e8e66d1c976fd966616f8ac294f5ddf4207a0253f631e
---

# Evaluation rigor

Bring the harness's statistics and tasks to what the field has learned. That means intervals
clustered by task, gates designed from their detectable effect, and the known flaws of HumanEval
checked. It also adds probes and comparisons that place lotml beside other languages built for
models.

## Why

- **The intervals are too narrow.** `harness/lotml_harness/agent/report.py` computes a Wilson
  interval over pooled runs, which is too narrow once a task runs more than once (`see n-0097`).
- **"Not significantly worse" proves little.** At about 200 pairs it cannot exclude a 10-point
  loss.
- **Some oracles are wrong.** EvalPlus found 18 defective canonical solutions in HumanEval, and
  adr:0015-pose-humaneval-untyped grades against those solutions' own results.
- **Comparisons already exist to join.** VeraBench measures three other languages built for
  models, and two questions lotml's data could answer for the first time are open: errors as
  values against exceptions, and the cost of type annotations.

Done when the reports cluster by task, the 18 tasks are checked, and each probe and comparison
below has a result. Every run that calls a model states its model list and cost ceiling before
it starts, within the spending caps the owner set. Third-party and model-written code runs only
through the harness's confined runner (`limits.py`, `confine.py`, `execute.py`). See
`docs/wiki/pages/evaluation-harness.md` and `docs/wiki/pages/language-design-evidence.md`.

## Paths

- `harness/lotml_harness/agent/report.py`
- `harness/lotml_harness/agent/licences.toml`
- `harness/lotml_harness/experiments/`
- `harness/lotml_harness/tasks/`
- `harness/results/`
- `docs/adr/`

## References

- adr:0002-errors-as-values
- adr:0011-proceed-to-phase-2-past-the-failed-phase-1-gate — gates already decided stand
- adr:0014-deepagents-over-openrouter-for-the-agent-harness
- adr:0015-pose-humaneval-untyped
- `research/llm-landscape/evaluation-and-adaptation.md`, `research/llm-landscape/languages.md`

## Out of scope

- Re-deciding any past gate (adr:0010, adr:0011, adr:0019).
- Training on any task set: `plans/seed-corpus-rebuild.md` owns what may be trained on.

## Tasks

- [ ] 1.1 (TDD) Report an interval clustered by task when a task has more than one run, a t-interval over per-task pass fractions, keeping the Wilson interval for one run per task
- [ ] 1.2 (TDD) Compute a planned gate's minimum detectable effect from the harness's own variances, for its number of paired tasks and samples per task, and print it with the gate's design
- [ ] 1.3 (Unit) State in the report of every gate written from now on a non-inferiority margin fixed before the run, and test against it
  _Depends 1.2_
- [ ] 2.1 (Unit) Check the 18 HumanEval tasks EvalPlus lists as having defective ground truth (arXiv 2305.01210) against the humaneval-original oracle, recording each task id, whether its oracle was wrong and what replaced it
- [ ] 2.2 (Unit) Report three numbers for each agent and phase run: the first answer, the answer after feedback, and whether the output was well formed
- [ ] 2.3 (Unit) Build an output-prediction probe from lotml programs, where the model predicts what a program prints and the confined runner executes it, measuring reading apart from writing
- [ ] 3.1 (Unit) Pose VeraBench's 60 problems in lotml from a pinned commit of
      `aallan/vera-bench`, keeping its MIT notice, registered in `licences.toml` as an
      evaluation set never trained on, and run them paired against Python on the phase 1
      gate's models
  _Depends 1.2_
- [ ] 3.2 (Unit) Split phase 1's recorded results into tasks that return an error and
      tasks that cannot, and report lotml's errors as values against Python's exceptions
      on each
  _Status removed_
  _Reason review of the literature on 2026-10-08: phase 1's tasks take MultiPL-E's signatures and none returns an error, so the side of the split that can fail is empty; replaced by 3.4_
- [ ] 3.3 (Unit) Measure Python with and without type annotations under the same
      harness, on the phase 1 gate's models, so the cost of annotating is measured
      rather than assumed
  _Status removed_
  _Reason review of the literature on 2026-10-08: annotation cost was already measured (MultiPL-E, p = 0.23; mame, 1.6-1.7x time), and phase 1's typed-Python rows ran on another interpreter than the harness now provisions; replaced by 3.5_
- [ ] 4.1 (Unit) Write a proposed ADR on isolating graded runs on Windows with an AppContainer profile (no network, writes confined to the scratch directory) on top of the Job Objects `limits.py` uses, for the owner to decide
- [ ] 1.4 (TDD) Compare two arms with more than one run per task by the per-task
      difference of pass fractions, with a paired standard error and a sign-flip
      permutation p-value, keeping exact McNemar for one run per task
  _Reason review of the literature on 2026-10-08: agent-harness-arms 1.2 asks for five runs per task and its group 3 for paired results, and 1.1 builds only a one-arm interval_
- [ ] 3.4 (Unit) Pose paired tasks whose specification states error cases — parse,
      validate, look up — and compare lotml's errors as values against Python's
      exceptions, with hidden tests on the error paths and the detectable effect
      computed before the run
  _Depends 1.2_
  _Reason review of the literature on 2026-10-08: replaces 3.2, whose split of phase 1 is empty_
- [ ] 3.5 (Unit) Measure Python with and without type annotations, both arms run fresh
      on the same recorded interpreter and the phase 1 gate's models, reporting pass@1,
      tokens, time and rounds against MultiPL-E's and mame's earlier measurements
  _Depends 1.2_
  _Reason review of the literature on 2026-10-08: replaces 3.3_
- [ ] 2.4 (Unit) Feed every stored model answer that `lotml check` accepts through the
      llguidance and GBNF dialects, report the share each rejects by cause, and widen a
      dialect for any cause above 1% — its indentation of four spaces only and its depth
      bound of eight among them
  _Reason review of the literature on 2026-10-08: 2606.21619 found a single changed lexeme cut a model from 62.5% to 1.9%, and the dialects are tested on canonical programs only (docs/wiki/pages/constrained-decoding.md)_

## Done when

- `uv --directory harness run pytest` passes, with tests for the clustered interval and the detectable-effect calculation.
- `harness/results/` holds the 18-task check, the probe, the VeraBench run and the two comparisons, each with its cost.
- The proposed ADR exists, and `scc validate` exits 0.
