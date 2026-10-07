---
status: superseded
superseded-by: 0021-compiler-in-rust-with-llvm-as-its-native-code-generator
---

# 0006 · Compiler written in Rust

## Context

The original study recommends writing the compiler in Rust, with a hand-written recursive-descent
parser (better error messages than parser generators give), a query-based incremental architecture
with Salsa and a separate tree-sitter grammar for editors. Salsa backs rust-analyzer with the
invariant the under-100 ms check target requires: editing a function's body does not invalidate
global data. The implementation language is among the most expensive decisions to change: Roc took
487 days to rewrite about 300,000 lines of Rust in Zig. See [[transpilation-strategy]].

## Decision

Write the compiler in Rust, with a hand-written parser, Salsa for incrementality, Cranelift for
debug builds and LLVM for release once the native backend arrives. Rejected for now: Zig (faster
builds of the compiler itself, but a smaller ecosystem for LSP, Salsa and Cranelift) and writing
the compiler in lotml itself (it does not exist yet).

## Consequences

- Roc's reason for leaving Rust was the compiler's own rebuild time (3.4 s against about 35 ms in
  Zig, on 450,000 lines): that cost falls on whoever develops the compiler and must be watched from
  the start, with the codebase split into crates.
- Salsa still labels itself "experimental" (0.28.5); its API may change.
- Cranelift's real gain in rustc was modest (about 5% of a clean build); the speed of the agent's
  loop depends more on the incremental frontend than on the backend.
