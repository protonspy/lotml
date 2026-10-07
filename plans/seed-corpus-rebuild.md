---
autonomy: auto
ci: wait
pr: per-group
merge: manual
status: approved
checksum: 60e7cc9f9e02f6c8b83f7e9b71f693ae62770cd154ac937aba07b77e66aa92e3
---

# Seed corpus rebuild

Rebuild the Python-to-lotml corpus from sources whose licences permit training, so the small
coder and the guide have a seed they may learn from. The corpus built from MultiPL-E's typed copies
stays as a measurement and nothing more.

## Why

`harness/results/corpus.md` translates MultiPL-E's typed copies of HumanEval and MBPP.
- MultiPL-E's licence forbids using its contents as training data.
- `harness/lotml_harness/agent/licences.toml` and `harness/results/NOTICE.md` already mark it so
  (`see n-0098`).

The wiki called that corpus the seed of training; this delivery corrects the page
(`docs/wiki/pages/small-coder-training.md`).

The originals are trainable: openai/human-eval is MIT, and MBPP's original release is CC BY 4.0.
`agent/humaneval.py` already poses both untyped, with signatures typed from the canonical
solution's run-time values (adr:0015-pose-humaneval-untyped).

A translation is model output as well as task text, so an item is trainable only when its source
and the model and providers that wrote it are all permitted. Today only the trace-dataset export
checks all three, through `Registry.decide`.

Done when a corpus built only from permitted sources exists and every item names its source,
licence, model and providers. Its export must go through `Registry.decide` and refuse anything
else.

## Paths

- `harness/lotml_harness/corpus/`
- `harness/lotml_harness/agent/humaneval.py`
- `harness/lotml_harness/agent/licences.toml`
- `harness/lotml_harness/agent/registry.py`
- `harness/lotml_harness/agent/dataset.py`
- `harness/results/`

## References

- adr:0015-pose-humaneval-untyped
- adr:0017-a-half-billion-coder-model-tuned-locally-and-served-by-llama-server
- `docs/wiki/pages/small-coder-training.md`
- `research/llm-landscape/evaluation-and-adaptation.md`

## Out of scope

- Training on the rebuilt corpus: that is the guide's and the small coder's own plans.
- Growing the corpus past the two originals, or adding EvalPlus's stronger tests to it.

## Tasks

- [ ] 1.1 (Unit) Register MBPP's original release (CC BY 4.0) in `licences.toml` with evidence, the date checked and its notice, and register the terms of each model the corpus pipeline translates with, default deny unchanged
- [ ] 1.2 (Unit) Load the original tasks through the typing `agent/humaneval.py` already does, with provenance stamped by the loader rather than carried on the item, and a test that no loaded prompt equals MultiPL-E's typed prompt for the same task
- [ ] 1.3 (Unit) Run the corpus pipeline on those tasks, writing a separate corpus whose every item names its source, licence, notice, model and providers
  _Depends 1.1, 1.2_
- [ ] 1.4 (Unit) Route the corpus's training export through `Registry.decide`, the single place an export is allowed, with tests refusing a MultiPL-E-derived item and an item from an unregistered model
  _Depends 1.3_
- [ ] 1.5 (Unit) Report how many tasks the rebuilt corpus keeps against the 509 of the MultiPL-E-derived one, and why each lost task was lost
  _Depends 1.3_

## Done when

- `harness/results/` holds the rebuilt corpus and its report, separate from the MultiPL-E-derived one.
- `uv --directory harness run pytest` passes, including the refusal and provenance tests.
- `scc validate` exits 0.
