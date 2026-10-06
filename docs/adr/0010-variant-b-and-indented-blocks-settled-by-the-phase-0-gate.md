---
status: accepted
---

# 0010 · Variant B and indented blocks, settled by the phase 0 gate

## Context

Two accepted records were conditional on the harness. adr:0004-python-syntax-where-semantics-match
adopted variant B on a pilot of 30 programs and promised a comparison with more models and
families before v1 froze the syntax. adr:0009-significant-indentation-with-symbol-addressed-edits
kept indented blocks on the condition that agents editing at scale do not fail more often than in
a braces form. The phase 0 gate (`harness/results/gate-0.md`) measured both, with Claude Haiku and
Sonnet through `claude -p` and two open families through Ollama (Qwen 2.5 Coder 7B, Llama 3.1 8B),
on 200 paired tasks from MultiPL-E's typed HumanEval and MBPP for the variants and on 192 paired
editing tasks. "Not worse" was read as: no model significantly worse (exact McNemar, two-sided, 5%)
and the pooled discordant pairs not favouring the alternative — an absence of evidence of harm,
not a showing that the forms are equivalent; the pooled counts are a direction check, not a test.

- **Variant B against variant A** ([[lotml-syntax]]): no model did significantly worse in B; Sonnet
  (21 tasks solved only in B, 1 only in A, p < 0.001) and Qwen (19 against 5, p = 0.007) did
  significantly better; Haiku (14 against 11) and Llama (6 against 9) did not differ; pooled, 60
  tasks only in B against 26 only in A.
- **Parsing and leakage with the reference:** Sonnet parsed all 350 of its variant B answers and
  leaked no Python construct.
- **Indented against braces** ([[editing-robustness]]): no model did significantly worse in the
  indented form; Claude fixed every task in both, Llama fixed 20 tasks only indented against 9 only
  with braces (p = 0.061), Qwen 3 against 6 (p = 0.51); pooled, 23 against 15. The braces form had
  more syntax slips (Llama: 28 turns against 8; Qwen: 3 against 0) and brought `else if` and `||`.

## Decision

Freeze variant B and significant indentation for v1: the conditions of 0004 and 0009 are met, and
neither choice is conditional any longer. The rest of what those records decided stands as written
— Python's syntax where the semantics match and visibly different syntax where they do not; a
tolerant parser, a canonical formatter with no options, and edits addressed to symbols. Rejected:
reopening variant A, which no model preferred significantly while two preferred B; and braces,
which the editing test gave no reason to adopt and which invited the C family's idioms.

## Consequences

- Changing the syntax is now a migration: the compiler, the reference, the corpus and every
  experiment after phase 0 are written in variant B with indented blocks.
- The variant A and braces machinery stays in the harness only to reproduce phase 0's results.
- What the gate did not measure stays open: frontier models other than Claude, agents with tools
  editing several files, and the effect of the compiler's diagnostics, which phase 1 measures.
