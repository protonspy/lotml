---
status: accepted
---

# 0020 · One IR between the checker and every backend

## Context

Two backends read the checked program today, each its own way. The Python backend
(`compiler/crates/lotml-py`) walks the syntax tree with the checker's types and writes a Python
syntax tree carrying lotml positions (adr:0001-transpile-to-python-first). The C backend
(`compiler/crates/lotml-c`) lowers the same tree to a typed intermediate form of its own —
monomorphic functions, every intermediate value named, structured blocks — then inserts counts,
reuse and uniqueness checks over it and writes C
(adr:0016-c-target-as-monomorphic-c-over-a-counting-runtime). That form lives inside the C crate
and names its functions as C does.

Every rule the language defines past the checker is therefore written twice: the overflow trap,
`/` on integers returning `f64` (adr:0007-integer-division-returns-f64), the copy on entering a
`var` or `inout`, errors as values, how a `match` takes a value apart. The 509-program parity
suite holds the two together by testing, not by construction. A native backend through LLVM
(adr:0021-compiler-in-rust-with-llvm-as-its-native-code-generator) would be a third reading of
the tree, or a second consumer of a form shaped for C. The project's direction is one pipeline —
syntax, checking, one IR, then each backend — so that what the compiler has proved about a
program reaches every target in the same form. See [[transpilation-strategy]] and
[[memory-model]].

## Decision

Add the crate `lotml-ir` between `lotml-check` and every backend, and let no backend read the
syntax tree. The IR is typed and structured: `if`, loops and `match` stay nested blocks rather
than basic blocks, every intermediate value is a named local, and every statement carries the
line it came from. Monomorphization, count insertion, reuse and uniqueness hoisting are passes
from IR to IR in the same crate, so the Python backend reads the program before them, with its
generics intact, and the C and LLVM backends read it after, monomorphic and counted. The C
backend's intermediate form is the starting point, moved rather than rewritten. Rejected: a
control-flow graph of basic blocks as the shared form, which suits LLVM but makes the Python
backend rebuild `if` and `while` from jumps, when the Python it writes is what `lotml run`
debugs — the native emitters build blocks from structured statements in one walk; keeping a
lowering per backend, which is three implementations of every rule above; and Python's `ast` as
the IR, which has no types and would have the native backends read Python.

## Consequences

- The C backend's lowering (about 3,900 lines) and its passes move crates, and the Python
  backend's emitter is rewritten against the IR. Both are held by the parity suite and by the C
  target's benchmarks, which must not move while the code does.
- The Python the backend writes gains temporaries where the IR names intermediate values; it is
  run and debugged through lotml positions rather than read, and that is the cost accepted.
- `lotml check` does not lower, so the under-100 ms check is untouched; building and running
  gain the lowering the Python target did not have.
- A rule the language adds is lowered once, and a backend that disagrees with another is a bug in
  that backend's emitter, not in a second lowering.
