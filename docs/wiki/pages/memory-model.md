# Memory model

The original study recommends mutable value semantics with reference counting and compile-time
elision, compiled AOT, as the middle ground between performance and ease for LLMs. The evidence
supports the choice, with three corrections: atomic counting is the hidden cost, inference alone
is not enough, and the language needs parameter conventions — the gap the
[[python-leakage-pilot]] exposed.

## Evidence for

- **Perceus** — Reinking, Xie, de Moura and Leijen,
  [PLDI 2021](https://xnning.github.io/papers/perceus.pdf). Reference counting with reuse: Koka's
  purely functional red-black tree (compiled to C) came within 10% of C++'s `std::map`; Java came
  close in time using almost 10 times the memory (1.7 GiB against 170 MiB). Without the counting
  optimizations Koka is more than 2 times slower, and they help less when data is heavily shared.
- **Counting Immutable Beans** — Ullrich and de Moura, [IFL 2019](https://arxiv.org/abs/1908.05647),
  the basis of Lean 4 (which emits C). Borrowed-parameter inference plus reset/reuse: Lean was 5
  times faster than OCaml on `const_fold`, spending 17% of its time deallocating against OCaml's
  90% in collection.
- **Mutable value semantics** — Racordon et al.,
  [JOT 2022](https://www.jot.fm/contents/issue_2022_02/article2.html). References are
  second-class: they exist only at call boundaries and are never stored; fixed-size values live
  on the stack and containers use copy-on-write. On 1,344 generated programs, Swift was the
  fastest in the "overwhelming majority" and only lost to C++ with more than 90% mutations.
- **Who adopted it:** Roc uses reference counting with Perceus; Nim uses ORC (counting with a
  cycle collector) by default since 2.0; Koka and Lean emit C with no collector.
- **Rust-style ownership weighs less than the original study suggests, but Rust does weigh.**
  Ownership and lifetimes are 16.7% of LLM compile errors in Rust
  ([Nogueira et al.](https://arxiv.org/abs/2608.00661)), and RustAssistant fixes those categories
  at the same rate as the others. Even so, compile errors are 94.8% of failures when translating
  to Rust ([RustRepoTrans](https://arxiv.org/abs/2411.13990)), and agentic cost in Rust was
  1.07–1.57 times Python's ([Tokenmaxxing](https://arxiv.org/abs/2607.22807)). What makes Rust
  hard for LLMs is the combination — traits, name resolution and ownership — and simplifying trait
  resolution matters as much as avoiding the borrow checker.

## The hidden costs

- **Atomic counting.** Making every counting operation atomic cost 5% to 59% in Perceus. In
  Swift, counting took 32% of execution time on average (42% in client programs), while 87–93% of
  the operations touched objects private to one thread
  ([Biased Reference Counting](https://iacoma.cs.uiuc.edu/iacoma-papers/pact18.pdf), PACT 2018).
  Free-threaded CPython uses the same biased-counting technique (PEP 703).
- **Inference stops at boundaries.** Swift had to add `borrowing`/`consuming` (SE-0377) and
  `~Copyable` types (SE-0390) because the optimizer cannot change conventions fixed by the ABI or
  optimize polymorphic interfaces.
- **Numbers that are not evidence:** the 95% of operations removed that Lobster announces are
  self-reported, with no methodology.

## Parameter conventions

Neither the original study nor the pilot spec says how a function changes a value that belongs
to its caller. In the pilot, Sonnet and Opus used the only documented path (methods with
`var self`); Haiku invented `var` parameters and wrote tests that expected the caller's value to
change — Python's reference semantics. With value semantics, those tests would fail.

The language needs explicit conventions, like Swift (`inout` with `&` at the call), Hylo and Mojo
(`mut`):

| convention | meaning | at the call |
| --- | --- | --- |
| default | read; the compiler borrows without copying | `f(x)` |
| `inout` | the function changes the caller's value | `f(&x)` — the mutation is visible |
| `sink` | the function takes ownership; the caller no longer uses it | `f(x)`, with the compiler forbidding later use |

Derived requirements: the marker at the call makes the mutation visible (the "no magic" and
"predictable performance" principles); `var self` and `inout` should be the same idea with one
word; and the [[semantic-compiler]] warns when a copied parameter is changed and discarded,
suggesting `inout` — the exact error the pilot showed.

## Cycles

Without stored references, cycles cannot form. Languages that allow stored mutable references pay
for it: Swift requires `weak`/`unowned`, Nim runs a cycle collector (ORC), Lobster reports cycles
at program exit. The original proposal ("forbid cycles by design; explicit weak references") is
aligned; graphs use the arena-and-index idiom, like the adjacency-dictionary graph in the paired
corpus. `weak` only comes in if a real case appears.

## Performance target

- **"1–2× C on numeric benchmarks" is plausible for numeric code:** Nim sits at 1.0–1.3× on most
  benchmarks and at 1.05× on kostya/benchmarks' `bench.b`.
- **Allocation-heavy code is where counting loses:** in the Benchmarks Game, Swift 6.0.3 ranges
  from 1.03× C (pidigits) to 10.5× (binary-trees) and 21.7× (regex-redux).
- **"10–100× faster than CPython"** is conservative: CPython 3.13 is 33× to 486× behind C++ on
  benchmarks that do not call a C library, and the 3.15 JIT adds 7–8% on average.

The project's own benchmark, which the study asks for, must include allocation-heavy programs,
not just numeric ones, and report them separately.

## Recommendations

1. Keep value semantics with reference counting, Perceus-style reuse and Lean-style borrow
   inference.
2. Explicit parameter conventions, with a visible marker at the call, already in v1 — the Python
   target needs them too ([[transpilation-strategy]]).
3. Non-atomic counting by default; values that pass between tasks are copied or moved, as the
   study already plans — see [[colorless-concurrency]].
4. No stored references; arena and indices in the standard library.
