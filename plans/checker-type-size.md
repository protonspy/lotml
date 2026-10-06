---
autonomy: auto
ci: wait
status: approved
checksum: 4d30e4999410d6b02ee44ed236c6a1dc86b0a6577cc47f444e35d429c6a43912
---

# Checker type size

Bound the size of the type the checker infers for an expression, so that a short program whose
types double line after line is reported instead of exhausting memory.

## Why

`Ty` is an owned tree, so `t1 = (t0, t0)` … `t30 = (t29, t29)` asks the checker for a type of
2^30 nodes: `lotml check` passed 500 MB within a second on a 32-line program, and so do
`lotml lsp` and `lotml mcp`, which check an agent's code on the developer's machine. The security
review of PR #16 found it there, where the C target got the same bound for generic instances.
Done when such a program gets one diagnostic in bounded memory, and nothing the suite checks today
changes.

## Paths

- `compiler/crates/lotml-check/src/body.rs` — `Body::expr`, where every expression's type is resolved and recorded
- `compiler/crates/lotml-diag/src/codes.rs` — the code table
- `compiler/crates/lotml-check/tests/types.rs`

## References

- https://github.com/protonspy/lotml/pull/16 — the C target's bound on generic instances, and the review that found this

## Out of scope

- Sharing type nodes, an interned or counted tree, which would make large types cheap rather than refused.

## Tasks

- [x] 1.1 (Unit) Report an expression whose type passes 65,536 nodes as E0222, explained in the code table, and check on with it as an error
- [x] 1.2 (TDD) Bound every resolution of a type at 1,024 nodes, so variables bound to
      each other cannot expand past it
  _Reason security review F1: the bound in Body::expr ran after resolve had expanded the type_
- [x] 1.3 (TDD) Bound the type nodes one function and one module store in all
  _Reason security review F2: thousands of mentions of a type under the limit still took gigabytes_
- [x] 1.4 (Unit) Write at most 64 nodes of a type in a message or a hover, eliding the
      rest
  _Reason security review F3: a type in a message was written whole_

## Done when

- `cargo test --manifest-path compiler/Cargo.toml -p lotml-check` passes, 18 doubling lines reporting E0222 alone and a tuple of 1,000 elements checking clean.
- `lotml check` on 30 doubling lines exits within a second, under 100 MB.
- `scc validate --checks --pr` exits 0.
