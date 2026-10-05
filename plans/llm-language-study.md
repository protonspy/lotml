---
autonomy: auto
ci: no-wait
---

# LLM language study

Deepen the study of the LLM-oriented programming language: check every claim against the
literature and our own measurements, test the proposed syntax with a pilot, and distill the
result into the repository's wiki.

## Why

The original study is a proposal with no measured evidence: the token targets, the risk of
"Python leakage" and the syntax choice are opinion until measured. This work turns the proposal
into a basis for decisions — verified concept pages, reproducible measurements in `research/` and
proposed ADRs for what is expensive to reverse. Done when the wiki covers the whole study, the
source in `docs/raw/` has been processed and the PR is open.

## Paths

- `docs/wiki/`
- `docs/adr/`
- `docs/glossary.md`
- `docs/stack.md`
- `research/tokens/`
- `research/pilot/`

## Out of scope

- Implementing the compiler or the transpiler.
- Closing the syntax decisions: the ADRs come out as `proposed`, and the decision is the project
  owner's.
- Fine-tuning models or benchmarking with paid third-party APIs.

## Tasks

- [x] 1.1 (Unit) Survey the verified literature and state of the art on the web: grammars for LLMs, low-corpus languages, constrained decoding, compiler in the loop, memory models
- [x] 1.2 (TDD) Write the multi-tokenizer token counter in `research/tokens/`, with a test that keeps special tokens out of the count
- [x] 1.3 (Unit) Build the typed-Python × proposal paired corpus and measure tokens on every tokenizer
  _Depends 1.2_
- [x] 1.4 (Unit) Draft the compact spec and the Lark grammar of both syntax variants in `research/pilot/`
- [x] 1.5 (TDD) Write the parse and Python-leakage checker and run the generation pilot with several models
  _Depends 1.4_
- [x] 2.1 (Unit) Record the study's canonical terms in `docs/glossary.md`
- [x] 2.2 (Unit) Write the concept pages in `docs/wiki/pages/` with the revised study and the results
  _Depends 1.1, 1.3, 1.5, 2.1_
- [x] 2.3 (Unit) Record proposed ADRs for the decisions that are hard to reverse
  _Depends 2.2_
- [x] 2.4 (Unit) Link the pages from the index, record the changelog and remove the source from `docs/raw/`
  _Depends 2.2, 2.3_

## Done when

- `scc validate` exits 0, with no `wiki.unprocessed-source`.
- The `research/` tests pass with `uv run`.
- Every numeric claim in the wiki cites a verified source or a measurement in `research/`.
- The PR is open against `main`.
