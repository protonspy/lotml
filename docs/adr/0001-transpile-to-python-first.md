---
status: accepted
---

# 0001 · Transpile to Python first

## Context

lotml does not exist yet, and what has to be validated first is whether LLMs write it well — not
whether it is fast. The pilot measured parsing and static rules, but without executing programs
there is no pass@1, which is the project's primary metric. Writing a native backend before the
syntax stabilizes is years of work thrown away at every change. Python gives, for free, a runtime,
the whole ecosystem and incremental adoption (calling lotml from Python), which is how BAML works
around an empty ecosystem. See [[transpilation-strategy]].

## Decision

lotml's first target is Python, through a Python AST emitted with the original source positions;
a minimal version enters in phase 0 so the harness can execute programs. C is the second target,
in phase 3, and the native backend (Cranelift and LLVM) comes only once the syntax is stable.
Rejected: starting with C (delays validation with LLMs and forces solving memory before syntax),
starting with a native backend (the worst of both), and Rust as a target (generated code full of
`Rc`/`clone` and slow compilation in the agent's loop).

## Consequences

- Semantics must be identical across targets, and Python does not help: arbitrary-precision
  integers require emulating the `i64` trap (about 2× on an addition, in a rough local
  measurement) and reference lists require a copy when entering a `var` or `inout`.
- Colorless concurrency has no faithful equivalent in Python; on the Python target, tasks become
  threads.
- The Python target's performance is irrelevant to the decision and cannot become an argument for
  skipping phase 3.
- Every call into a Python library is fallible (`T ! PyError`), because stubs do not declare
  exceptions.
