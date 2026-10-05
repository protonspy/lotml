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

### Design

- [[lotml-syntax]] — the syntax rule, variants A and B, gaps, indentation
- [[type-system]] — types checkable on prefixes, optionals, overflow, effects
- [[memory-model]] — value semantics, reference counting, parameter conventions
- [[colorless-concurrency]] — green threads and where colorless runtimes leak

### Tools

- [[semantic-compiler]] — diagnostics, the repair loop, digest, LSP and MCP
- [[constrained-decoding]] — grammars for constrained generation and type constraints

### Plan

- [[transpilation-strategy]] — Python and C targets, the Python→lotml corpus
- [[evaluation-harness]] — metrics, sample size and gates
- [[lotml-risks]] — the revised risk table
- [[requirements-and-roadmap]] — revised and new requirements, phases and gates
