---
status: superseded
superseded-by: 0010-variant-b-and-indented-blocks-settled-by-the-phase-0-gate
---

# 0004 · Python syntax where semantics match

## Context

The original proposal (variant A) uses `none`, `match` arms without `case`, `=>`, `use`, and
`x or default` and `if x:` for optionals. Two of those forms repeat Python's syntax with different
semantics: in Python, `or` and `if` treat `0`, `""` and `[]` as false; in variant A, only absence
counts. The parser does not catch that difference. The others diverge from Python with no
semantics to justify it. Variant B replaces those six constructs with Python's or with
unambiguous forms (`??`, `is not None`). Measured: B costs 0.8 percentage points more tokens; in
the pilot, A and B tied on parsing (28 of 30) and on syntactic leakage (zero). The syntax is what
all code, all docs and every corpus will use — changing it after v1 is a migration. See
[[lotml-syntax]].

## Decision

Adopt variant B: `None`, `case` in `match` arms, `lambda`, `from … import`, `??` as the optional
default, `is None`/`is not None` as the optional test and `Err(e)` to compare with an error in
tests. The general rule: semantics equal to Python's use Python's syntax; different semantics use
visibly different syntax. Accepted on 2026-10-05; the harness repeats the comparison with more
models and families before v1 freezes.

## Consequences

- The optional test costs 3 more tokens (`is not None` against `if x:`).
- `??` is the only new form, and it comes from C#, Swift, Kotlin and JavaScript, not Python.
- The transpiler to Python gets more direct: `None`, `lambda`, imports and `match` pass through
  almost untranslated.
- If the harness shows variant A with fewer semantic errors, a new ADR supersedes this one and the
  general rule is revisited.
