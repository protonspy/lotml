# Wiki

The entry point. Every page under `wiki/pages/` has to be reachable from here —
directly, or through a page that is — because a page nothing links to is a page
nobody will find again.

Pages link to each other as `[[page-slug]]`, where the slug is the filename without
its extension and without its directory. A link that resolves to no page is reported,
and so is a page this index cannot reach.

This file and `changelog.md` live here rather than in `pages/`: they are the wiki's
fixed documents, not pages, and neither is ever an orphan.

## Pages

### lotml — study

- [[llm-oriented-language]] — the study's entry page: conclusions, verdict on the original proposal, accepted decisions

### What was measured

- [[token-cost]] — token cost of the syntax on eight tokenizers, paired corpus and isolated constructs
- [[python-leakage-pilot]] — three models writing variants A and B from the spec, parsed and executed
- [[editing-robustness]] — slips in both block styles, the editing pilot and the edit-format literature
- [[source-verification]] — the downloaded papers, the quotes checked against them and the corrections

### Context

- [[training-prior]] — languages without a corpus, confusion with Python and how to get around it
- [[languages-for-agents]] — languages already designed for LLMs and what none of them measured
- [[language-design-evidence]] — what was measured about types, indentation, errors as values and intermediate languages for model-written code
- [[agent-harness-design]] — how coding harnesses are built, how much a harness moves a fixed model, and what lotml measures

### Design

- [[lotml-syntax]] — the syntax rule, variants A and B, gaps, indentation
- [[type-system]] — types checkable on prefixes, optionals, overflow, effects
- [[memory-model]] — value semantics, reference counting, parameter conventions
- [[colorless-concurrency]] — green threads and where colorless runtimes leak

### Tools

- [[semantic-compiler]] — diagnostics, the repair loop, digest, LSP and MCP
- [[constrained-decoding]] — grammars for constrained generation and type constraints
- [[compiler-embedded-model]] — prior work on a small model inside the compiler: repair, explanations, lints

### Compiler

- [[prior-art-compilers]] — sixteen projects that compile or run Python, read against lotml's compiler: verdict, what is rejected, what may be copied
- [[target-parity]] — proving `lotml run` and `lotml build` agree: floors, fuzzing, every test in every mode
- [[python-interop]] — a program calling Python through `py.<module>`: what each kind of value becomes, what can fail, what is still out
- [[compiler-performance]] — where checking, building and running spend time, and what the prior art does about it

### Training a small coder

- [[small-coder-training]] — distillation, then reinforcement learning: what works at 0.5B–1.5B and for a language no teacher knows
- [[grpo]] — the algorithm, its variants, what goes wrong and the settings small runs used
- [[verifiable-rewards]] — rewards from the compiler and the tests, for writing and for repair
- [[rl-environment]] — the tests, task pools, grader and evaluation around a training run
- [[reward-hacking]] — how a policy games its grader, and the defences
- [[repair-training]] — models that fix code and point where: agents, localizers, seeded and real bugs

### Plan

- [[transpilation-strategy]] — Python and C targets, the Python→lotml corpus
- [[evaluation-harness]] — metrics, sample size and gates
- [[lotml-risks]] — the revised risk table
- [[requirements-and-roadmap]] — revised and new requirements, phases and gates
