---
autonomy: auto
ci: wait
---

# Small coder training

Record in the wiki what the evidence says about training a small coder model — distillation,
GRPO-style reinforcement learning, reward design, the training environment and bug-fixing models —
with every number checked against its source.

## Why

The guide (adr:0017-a-half-billion-coder-model-tuned-locally-and-served-by-llama-server) is tuned
by supervised fine-tuning alone, and the next question is whether reinforcement learning with the
compiler and tests as the verifier, or distillation from a larger teacher, would do better for a
0.5B-1.5B model on the project's budget. A deep-research survey answered it from about a hundred
sources, but its numbers came from reading passes and search summaries. Done when the open-access
papers behind the numbers the wiki uses are in `research/literature/`, every such number is pinned
to a verbatim quote that `verify.py` finds, and wiki pages say what the evidence implies for lotml
and what it leaves open.

## Paths

- `research/literature/`
- `docs/wiki/`
- `docs/glossary.md`

## References

- `docs/wiki/pages/compiler-embedded-model.md` — the prior survey of a small model inside the compiler
- `docs/wiki/pages/training-prior.md` — languages without a corpus and the confusion with Python
- `docs/wiki/pages/source-verification.md` — how sources and quotes are checked
- adr:0017-a-half-billion-coder-model-tuned-locally-and-served-by-llama-server — the guide as it is trained today

## Out of scope

- Training a model, or deciding to: the pages record the evidence and what it implies; a decision
  to run reinforcement learning or distillation is an ADR of its own.
- Machine-checking sources with no open PDF (blogs, documentation, model cards, product pages):
  they are cited by link, as the source-verification page already does for non-paper sources.

## Tasks

- [x] 1.1 (Unit) Add the arXiv papers behind the new pages' numbers to `sources.json` with versioned URLs and fetch them
- [x] 1.2 (Unit) Read each new source in full and record its claims with verbatim quotes in `claims.json` until `verify.py` finds every quote
  _Depends 1.1_
- [x] 1.3 (Unit) Write the wiki pages on small-coder training, GRPO, verifiable rewards, the RL environment, reward hacking and repair training from the verified claims, and link them from the index
  _Depends 1.2_
- [x] 1.4 (Unit) Update the source-verification counts, the glossary and the changelog
  _Depends 1.3_

## Done when

- `scc validate` exits 0.
- `uv run --with pytest==9.1.1 pytest` in `research/literature/` passes and `uv run python verify.py`
  reports every quote found.
- The new wiki pages are reachable from `docs/wiki/index.md`, and every number they take from a
  paper in `sources.json` has a claim in `claims.json`.
