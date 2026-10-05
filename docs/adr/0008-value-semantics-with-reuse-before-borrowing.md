---
status: accepted
---

# 0008 · Value semantics with reuse before borrowing

## Context

adr:0003-value-semantics-with-reference-counting chose mutable value semantics with reference
counting, Perceus-style reuse and Lean-style borrow inference. Checking its evidence against the
papers ([[source-verification]]) corrected two of its readings. Koka's red-black tree within 10% of
C++ is the best case for reuse — its trees are never shared — and the authors read all their
results as evidence of viability, not as absolute comparisons. Lean's lead over OCaml does not
credit borrow inference: turning borrow inference off made `const_fold` faster (0.90 of base),
while turning reuse off made it 1.64 times slower, and the paper puts the gap down mainly to
OCaml's collection time. Borrowing and reuse compete: a borrowed parameter saves an increment and a
decrement but can never be reused in place, which is why Perceus has no borrowing. Reuse is
fragile when values are shared, and allocation-heavy code is where counting loses (Swift: 10.5× C
on binary-trees). The rest of 0003's case stands: no pauses, no visible borrow checker, no cycles
when references are never stored, and atomic counting as the hidden cost (5–59% in Perceus). See
[[memory-model]].

## Decision

Keep mutable value semantics: assigning or passing is conceptually copying, the compiler turns it
into a move when it proves that safe, and collections use copy-on-write. Manage memory by reference
counting with Perceus-style reuse as the first optimization; add borrow inference only for the
cases where the benchmark suite shows it pays, since every borrowed parameter is a reuse given up.
Counting is non-atomic within a task, and a value handed to another task is marked shared once,
recursively, as Koka and Lean do — not biased counting, which assumes objects stay on the thread
that allocated them. References are never stored, so there are no cycles. Parameter conventions
(default, `inout`, `sink`) are explicit, with a visible marker at the call, from v1. Rejected, as in
0003: a tracing collector, Rust-style ownership with a borrow checker, and manual management.

## Consequences

- The C target ships reuse first; borrow inference is a later optimization behind a measurement,
  not a v1 promise.
- Benchmarks report numeric, allocation-heavy and sharing-heavy programs separately, because the
  headline numbers are best cases and sharing is where reuse stops firing.
- Graphs and structures with sharing use arenas and indices.
- `inout` with a marker at the call is new syntax for the model; without it, the pilot showed a
  model inventing reference semantics.
- On the Python target, value semantics is emulated by copying when a value enters a `var` or an
  `inout`.
