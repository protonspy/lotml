---
status: accepted
---

# 0016 · The C target as monomorphic C over a counting runtime

## Context

Phase 3 adds the C target (R18, R19): the same results as the Python target, memory managed as
adr:0008-value-semantics-with-reuse-before-borrowing decides — counting with Perceus-style reuse,
non-atomic counts, values marked shared when handed to a task — and within 2× C on numeric
benchmarks. Three facts shaped the backend. Perceus decides where a count goes up or down from
each variable's last use, so it needs liveness over explicit operands, which a syntax tree does not
give. The 2× target rules out boxing every number: a `[f64]` of boxed floats allocates on every
store, and the numeric benchmarks are loops over lists of numbers. And "the same results" means
CPython's: `repr` of floats and strings, records printed as their dataclass, sets in the order of
CPython's hash table. The machine the work is done on has only Visual Studio's compiler; CI runs
Ubuntu with gcc. See [[memory-model]] and [[transpilation-strategy]].

## Decision

The backend (`compiler/crates/lotml-c`) lowers the checked program to a small typed intermediate
form, one copy of each generic function and type per set of type arguments it is used with, with
every intermediate value named. A pass over that form inserts the counts: every parameter but
`inout` is owned, a variable's last use moves it and an earlier one increments, a value is
decremented where it dies, and a `match` arm that takes apart a value it alone holds builds its new
value in the old one's memory. C is written from that form with `#line` directives. Numbers,
`bool`, tuples, optionals and results are C values; strings, collections, records, variants and
closures live on the heap with a 32-bit count, positive when private to a task, negative once
marked shared and changed atomically from then on, zero for static values that are never freed.
The runtime is C, compiled into each program as one translation unit, and serves collections
through type descriptors (size, copy, drop, equality, order, hash, text); it reproduces CPython's
float `repr`, string `repr`, format mini-language, `hash` under `PYTHONHASHSEED=0` and set table
order. The C compiler is the first of `LOTML_CC`, `CC`, `cc`, `gcc` and `clang` found, then Visual
Studio's `cl` located by the `find-msvc-tools` crate; integer checks use `__builtin_*_overflow`
where the compiler has it and portable comparisons where it does not.

Rejected: boxing every value behind one tagged word, as Lean's polymorphic code does (one
representation for generics, but an allocation per float stored); generating C straight from the
syntax tree (no last use to move on, so every argument is incremented and reuse never fires); a
tracing collector and C++ as the target (adr:0008 and adr:0001-transpile-to-python-first); and
requiring one compiler (gcc is not on Windows by default, `cl` is not on Linux).

## Consequences

- A generic function is compiled once per use; a program with many instantiations compiles to more
  C, which the C compiler pays for.
- Records and variants are heap cells, so a field assignment through a `var` copies the cell when
  another binding holds it; small records stored inline are a later optimization behind a
  benchmark, as borrowing is.
- The runtime carries copies of CPython algorithms (float `repr`, SipHash-1-3, the set table) whose
  only purpose is identical output; a CPython change in them is a parity failure the suite reports.
- `inout` is a pointer to the caller's slot, so a callee changing it changes the caller's value in
  place when the value is unique, and copies it once when not.
- A Python module cannot be imported on the C target; C libraries are called directly.
