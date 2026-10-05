---
status: accepted
---

# 0002 · Errors as values

## Context

Exceptions leave the error flow out of the signature: a caller cannot tell, without reading the
body, whether and how a function fails. For an LLM that means guessing. Practitioners report that
agents "are afraid of" exceptions and prefer typed results, and every recent language built for
agents (BAML, NanoLang, Zero) adopted typed errors — without, however, publishing measured
evidence ([[languages-for-agents]]). The decision shapes every standard-library API and every
binding: changing it later means rewriting the ecosystem. See [[type-system]].

## Decision

lotml has no exceptions. A function that can fail declares `T ! E` (sugar for `Result[T, E]`),
fails with `fail e` and propagates with `expr?`; `x ?? fail e` (written `x or fail e` in variant
A; see adr:0004-python-syntax-where-semantics-match) turns absence into an error. Panics exist only
for broken invariants (index out of range, `assert`). Rejected: checked exceptions as in Java (the
same information, with more ceremony and no composition) and free exceptions as in Python (what
the project wants to eliminate).

## Consequences

- Fallible signatures cost about 3 more tokens than the Python version with a hidden exception,
  and each propagation costs 1 (`?`) ([[token-cost]]).
- The model tends to write `raise`/`try`; the pilot did not see that with the spec in the prompt,
  but the compiler must recognize those habits and suggest the fix.
- Calls into Python are all fallible (`T ! PyError`).
- A unit type has to be defined for functions that fail without returning a value (R28).
