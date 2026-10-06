---
autonomy: auto
ci: wait
---

# Agent data

Scale the agent harness from its eight hand-written tasks to the original HumanEval's 164, and turn
what the agent does on them into training data for the compiler-embedded model: its passing
trajectories, and the repairs it makes when `check` refuses its code.

## Why

The agent benchmark exercises the harness but cannot settle anything: a 10-point difference
between its arms needs 168 paired tasks (docs/wiki/pages/evaluation-harness.md). HumanEval was
written for Python, which lotml's syntax follows, and its original release is MIT-licensed, so its
tasks can be both measured and trained on — unlike the task set's copy, which comes through
MultiPL-E and may not be trained on. The same runs are the data the wiki says a compiler-embedded
model needs — refused answers, the checker as critic, repairs that check
(docs/wiki/pages/compiler-embedded-model.md). Done when every HumanEval task the translator keeps
has run in both arms with its report committed, and a dataset has been exported from it holding
only sources and models whose terms allow training, each with its evidence.

## Paths

- `harness/lotml_harness/agent/` — the agent harness these extend
- `harness/lotml_harness/tasks/` — `sources.download` and `values.render`, which the HumanEval tasks reuse
- `harness/results/` — the rows, the reports and the dataset's manifest
- `harness/cache/agent/` — the traces the dataset is exported from

## References

- `specs/agent-humaneval/` — the original HumanEval's problems as agent tasks, sampled and graded like the benchmark's
- `specs/trace-dataset/` — the licence registry, and trajectories and repairs exported from the traces
- `specs/agent-harness/` — the harness both build on
- `plans/compiler-embedded-model.md` — the study of small repair models these runs are data for
- `docs/wiki/pages/compiler-embedded-model.md` — what that study concluded: the compiler as critic and source of data, repair over authoring
- adr:0014-deepagents-over-openrouter-for-the-agent-harness
- adr:0015-pose-humaneval-untyped — the agent writes the types; 30 of the original's 164 functions carry them
- `harness/results/NOTICE.md` — why the task set's MultiPL-E HumanEval may not be trained on

## Out of scope

- Training the compiler-embedded model: the roadmap sets training aside as its own initiative;
  this produces the data and stops there.
- MBPP, LiveCodeBench and MultiPL-E's translations. A grant to train on MultiPL-E and
  LiveCodeBench was reported in conversation on 2026-10-06; until it is in writing from their
  holders, the registry keeps them out. MBPP's CC BY 4.0 original is the next source if more tasks
  are needed.
- The phase 1 gate's refused answers, real errors the study rates highest: they answer MultiPL-E's
  prompts, so `harness/results/NOTICE.md` keeps them out of training.

## Tasks

- [ ] 1.1 (Unit) Record in the registry whether Z.ai's and the serving providers' terms allow training on `z-ai/glm-5.3-flash`'s outputs, with the link to each
- [ ] 1.2 (Unit) Record in this plan's `## Why` how many HumanEval tasks were kept and how many cases they hold, as `harness/results/agent-humaneval.md` reports it
- [ ] 2.1 (Unit) Run every kept HumanEval task in both arms on `z-ai/glm-5.3-flash`, with traces carrying the check snapshots, and commit the rows and the report
  _Depends 1.2_
- [ ] 2.2 (Unit) Run the agent benchmark again in both arms, so its traces carry the check snapshots, the system message and the compiler version the export needs
- [ ] 2.3 (Unit) Export the dataset from the HumanEval and benchmark runs with the exporter, and record in the manifest how many trajectories and repairs each source gave
  _Depends 1.1, 2.1, 2.2_

## Done when

- `harness/results/agent.md` reports both arms on every HumanEval task kept,
  with McNemar's comparison.
- `harness/results/dataset.md` lists only sources and models the registry permits, each with its
  evidence and notice. If the model's terms forbid training or cannot be established, a manifest
  that exports nothing and says why is the finished result, and the next model is a new plan.
- `scc validate` reports no findings.
