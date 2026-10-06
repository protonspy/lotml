---
status: superseded
superseded-by: 0008-value-semantics-with-reuse-before-borrowing
---

# 0003 · Value semantics with reference counting

## Context

lotml needs predictable performance without the difficulty of a visible borrow checker and
without a collector's pauses. The memory model defines the runtime, the FFI, concurrency and what
the programmer (and the model) has to reason about — changing it later means redoing the
compiler. The evidence: Perceus brought Koka within 10% of C++ on a red-black tree; Lean 4 with
reuse and inferred borrowing was 5 times faster than OCaml on a benchmark; mutable value semantics
with second-class references eliminates cycles by construction. The costs: atomic counting cost
up to 59% in Perceus, and Swift had to put parameter conventions in the language because inference
stops at ABI boundaries. See [[memory-model]].

## Decision

Mutable value semantics: assigning or passing is conceptually copying; the compiler turns it into
a move or a borrow when it proves that safe, and collections use copy-on-write. Memory is managed
by reference counting with Perceus-style reuse and Lean-style borrow inference, non-atomic
counting within a task, and values copied or moved between tasks. References are never stored, so
there are no cycles. Explicit parameter conventions (default, `inout`, `sink`) with a visible
marker at the call. Rejected: a tracing collector (pauses and a bigger heap), Rust-style ownership
with a borrow checker (more compile rounds for the model) and manual management.

## Consequences

- Graphs and structures with sharing use arenas and indices.
- `inout` with a marker at the call is new syntax for the model; the pilot showed that without it
  a model invents reference semantics, so the convention enters in v1.
- Allocation-heavy code tends to be slower than numeric code (Swift: 10.5× C on binary-trees);
  benchmarks must report that class separately.
- On the Python target, value semantics is emulated by copying when a value enters a `var` or an
  `inout`.
