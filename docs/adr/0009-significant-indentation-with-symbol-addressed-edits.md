---
status: accepted
---

# 0009 · Significant indentation with symbol-addressed edits

## Context

adr:0005-significant-indentation kept indentation-based blocks on the condition that agents do not
fail more often editing them than editing braces, and justified the risk with SWE-agent needing a
guard that reverts edits with indentation errors. Checking the paper ([[source-verification]])
showed that guard is a general lint gate: it reverts an edit introducing any of eight flake8 codes,
three of them indentation, and the failed edits were never broken down by code, so indentation's
share is unknown. What was measured since points at the edit interface more than at the block
style ([[editing-robustness]]): edits addressed to named syntax entities, re-indented by the tool
and rejected when they break the syntax, cut edit errors by 76–88% on SWE-bench Verified, all
Python. The editing pilot passed 36 of 36 edits in the indented form and 35 of 36 in a braces form,
on far fewer than the 168 paired tasks a decision needs. A misplaced `}` is silent 7.1% of the time
against 2.9% for an indentation slip that still parses. After v1, changing the block form means
migrating all existing code. See [[lotml-syntax]].

## Decision

Keep indentation-based blocks, with a tolerant parser that reports wrong indentation with the fix,
a canonical formatter with no options, and edits addressed to symbols — a function body, a `match`
arm, a method — that take the indentation from the target and are rejected if they break the
syntax. The decision stays conditional on the harness's editing test at scale: if agents applying
edits fail more often in the indented form than in a braces form, a new ADR supersedes this one and
v1 ships with explicit delimiters. Rejected: braces now, which give up Python's prior and invite the
C family's idioms on evidence that does not separate the designs; and indentation without a
structured edit interface, which leaves whitespace as the thing a text edit can get wrong.

## Consequences

- The harness needs editing tasks — long files, multi-turn agents, open models, strict and tolerant
  application — before the phase 0 gate.
- The semantic compiler owes symbol-addressed edits (R32) and a check on every edit against the
  pre-edit state.
- The grammar published for constrained generation needs dialects that accept indentation; in
  llguidance's Lark, blocks are line-oriented and depth-bounded.
- The transpiler to Python stays trivial in this respect.
