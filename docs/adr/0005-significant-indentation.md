---
status: accepted
---

# 0005 · Significant indentation

## Context

lotml inherits Python's indentation through the training prior, and replacing indentation with
braces costs only one token per block. The pilot saw no indentation error in 60 programs — but
programs written in one go. Agents work by editing: SWE-agent needed a guard that reverts edits
with indentation errors, aider built relative-indentation patching (without flexible patching, 9
times more editing errors), and the Lark subset OpenAI accepts for constrained generation cannot
express `INDENT`/`DEDENT`. After v1, changing the block form means migrating all existing code.
See [[lotml-syntax]] and [[constrained-decoding]].

## Decision

Keep indentation-based blocks, with a canonical formatter and a tolerant parser that reports wrong
indentation with the fix, **conditional** on the harness's editing test: if the indented variant
fails more often than a braces variant in edits applied by agents, a new ADR supersedes this one
and v1 ships with explicit delimiters. Rejected for now: deciding in either direction without
measuring.

## Consequences

- The harness needs editing tasks (search-and-replace and diff) before the phase 0 gate.
- The grammar published for constrained generation needs dialects that accept indentation (GBNF
  and EBNF with their own lexer); in OpenAI's Lark subset, constraining requires an alternative
  form with block markers.
- The transpiler to Python stays trivial in this respect.
