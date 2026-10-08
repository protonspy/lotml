---
autonomy: auto
ci: wait
branch: feat/guide-evaluation
delivery: merged
pr: 23
---

# Guide evaluation — requirements

## Purpose

Before any agent runs with the harness guide, it is measured alone: on failures agents really made,
on problems it never trained on, against what the compiler already says (plans/harness-guide.md).
The study behind it is why each part is here. Seeded bugs trained a detector whose accuracy on
seeded bugs said little about real code (DeepBugs), and HDLdebugger's figures were on its own
generator's split — so only real failures count. DrRepair found the line the compiler reports is
often not the one to fix — so the compiler's location is the baseline to beat. Unchecked guidance
was right about half the time, and PyFiXV bought precision with silence — so silence and precision
are both reported (docs/wiki/pages/compiler-embedded-model.md).

## R1 · The failures

- **R1.1** The guide evaluation shall take failures only from held-out problems and only real ones, from any model: the repairs in agent-harness traces, by `check` or by test, as the trace dataset extracts them, and each lotml answer of the phase 1 gate that failed and was followed by a passing answer in the same conversation.
- **R1.2** If a failure's problem is not held out, then the guide evaluation shall refuse it.
- **R1.3** If the guide's configuration names no training records, or records that cannot be read, that lack a problem or a split, or that hold a problem `split.py` derives as held out, then the guide evaluation shall refuse to run.
- **R1.4** The guide evaluation shall need at least 97 failures with a fix, so that top-1's 95% Wilson interval is no wider than ±10 points, and with fewer shall report the intervals it has, how many more it needs, and no verdict.

## R2 · What is measured

- **R2.1** The guide evaluation shall ask the guide about each failure through `lotml guide ask` on a scratch copy of the failing file, the path the agents' tool takes, and record the answer and its latency.
- **R2.2** The guide evaluation shall score top-1 and top-3 location as a predicted declaration among those the real fix changed, as `lotml dev diff` reports them.
- **R2.3** The guide evaluation shall score the same locations for the compiler's baseline: the declarations holding the first three diagnostics or, for a failing test, the function its block calls first.
- **R2.4** The guide evaluation shall report how often the guide stays silent, its top-1 precision over the answers it gives, and, of the edits the guide proposed, the share shown and the share withheld by each reason.
- **R2.5** The guide evaluation shall report latency at the median and the 95th percentile and the guide server's peak memory, with the CPU, the model file and its quantization.
- **R2.6** The guide evaluation shall break every score down by kind of failure — `check` or test — and by the model that failed.
- **R2.7** The guide evaluation shall commit `harness/results/guide.md`, with the digest of the training records the guard read, and one row per failure under `harness/results/guide/`, holding only the problem, origin, model, the guide's and the baseline's locations as symbols, the scores, the reasons from the guide tool's fixed list and the latency, scrubbed as the agent harness scrubs what it writes there.
- **R2.8** The guide evaluation shall write each failure's file into its scratch directory through the agent harness's safe layer, and run every `lotml` call with the harness's clean environment, its files after `--`, a memory cap, and a deadline that ends the call's whole process tree.

## Out of scope

- Seeded failures as evaluation data: they are training data, and scoring on them would measure the
  mutator.
- Agents using the guide: that is the guide arms' spec (plans/harness-guide.md 3.2).
