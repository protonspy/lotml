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
- `harness/lotml_harness/tasks/` — the translator the HumanEval tasks go through
- `harness/results/` — the rows, the reports and the dataset's manifest
- `harness/cache/agent/` — the traces the dataset is exported from

## References

- `specs/agent-humaneval/` — the original HumanEval's problems as agent tasks, sampled and graded like the benchmark's
- `specs/trace-dataset/` — the licence registry, and trajectories and repairs exported from the traces
- `specs/agent-harness/` — the harness both build on
- `plans/compiler-embedded-model.md` — the study of small repair models these runs are data for
- `docs/wiki/pages/compiler-embedded-model.md` — what that study concluded: the compiler as critic and source of data, repair over authoring
- adr:0014-deepagents-over-openrouter-for-the-agent-harness
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
- [ ] 2.1 (Unit) Run every kept HumanEval task in both arms on `z-ai/glm-5.3-flash`, and commit the rows and the report
- [ ] 2.2 (Unit) Export the dataset from the HumanEval run and the agent benchmark's, and record in the manifest how many trajectories and repairs each source gave
  _Depends 1.1, 2.1_

## Done when

- `harness/results/agent.md` reports both arms on every HumanEval task the translator keeps,
  with McNemar's comparison.
- `harness/results/dataset.md` lists only sources and models the registry permits, each with its
  evidence and notice, and `scc validate` reports no findings.
