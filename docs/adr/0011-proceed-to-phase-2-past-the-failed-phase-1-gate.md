---
status: accepted
---

# 0011 · Proceed to phase 2 past the failed phase 1 gate

## Context

The roadmap makes each phase conditional on a measured gate: "if it does not pass, the next phase
does not start and the syntax is revisited" ([[evaluation-harness]]). The phase 1 gate asked for
lotml's pass@1 to be no lower than typed Python's on the same tasks, a median of at most 2 rounds
to green, and no more tokens than typed Python. Its run (`harness/results/phase1.md`, four models,
200 paired tasks each, up to three answers with `lotml check` and the hidden tests as feedback)
passed two of the three and failed pass@1:

- **Rounds and tokens pass** — a median of 1 round to green for every model, and lotml/Python
  program tokens at 0.99, the median of models.
- **Pass@1 fails** — Sonnet 97.0% against 97.5% (p = 1.000), Haiku 84.0% against 92.0%
  (p = 0.001), Qwen 2.5 Coder 7B 45.5% against 83.5% and Llama 3.1 8B 19.0% against 65.5% (both
  p < 0.001).
- **The gap is answers the compiler refused, not wrong logic.** Every model's first lotml answer
  failed the hidden tests less often than its Python one; the shortfall is first answers `lotml
  check` rejected — Haiku 20, Sonnet 4, Qwen 91, Llama 143 of 200. The frontier models were refused
  mostly for mutability and unknown names and fixed nearly all of it from the diagnostics (Haiku
  98.5% and Sonnet 100% solved after feedback); the 7–8B models also wrote syntax lotml does not
  have and mostly did not recover.

That is the "no training corpus" risk of [[lotml-risks]] measured, and phase 2 carries the
roadmap's planned answer to it: the Python→lotml corpus pipeline, alongside the LSP and MCP
server, the stub-based bindings and concurrency.

## Decision

Proceed to phase 2 as planned, with the phase 1 gate recorded as failed rather than reinterpreted:
its result and its criterion stay as they are, and this record overrides only the consequence that
the next phase does not start. The syntax is not revisited, since frontier models were refused for
semantics the compiler enforces (mutability, names) rather than for syntax, and adr:0010-variant-b-and-indented-blocks-settled-by-the-phase-0-gate
stands. Not taken: re-measuring with a changed reference or prompt as a separately recorded run,
which would have kept the gate binding at the cost of tuning the prompt against the tasks it is
scored on; revising the criterion to frontier models only, which would rewrite a gate after seeing
its result; and stopping the roadmap at phase 1, which leaves the corpus that addresses the measured
gap unbuilt.

## Consequences

- Phase 2 is built on a language that, read from its reference alone, trails typed Python at pass@1
  for three of the four models measured; its tools, corpus and bindings are written against that.
- The pass@1 gap stays an open, measured risk rather than a closed question. It is seen again
  wherever the harness re-runs phase 1's experiment — after the corpus exists, or with the
  measurements of task 3.6 — and a rerun is recorded as a new run beside the original, never in
  its place.
- The gates after this one keep their force: overriding phase 1 sets no precedent for phase 2's
  "corpus validated by tests" or phase 3's parity across targets, each of which is an executable
  check rather than a comparison against Python.
- Small open models remain the weakest audience for lotml until a corpus or fine-tuning reaches
  them; training is out of the roadmap's scope.
