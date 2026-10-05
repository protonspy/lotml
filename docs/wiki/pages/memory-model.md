# Memory model

The original study recommends mutable value semantics with reference counting and compile-time
elision, compiled AOT, as the middle ground between performance and ease for LLMs. The evidence
supports the choice, with four corrections: the headline benchmarks are best cases, atomic
counting is the hidden cost, borrowing and reuse compete rather than add up, and the language
needs parameter conventions — the gap the [[python-leakage-pilot]] exposed and the executed
pilot confirmed. Every number taken from a paper in `research/literature/sources.json` is quoted in
`claims.json` there and checked against the paper by `research/literature/verify.py` ([[source-verification]]).

## Evidence for

- **Perceus** — Reinking, Xie, de Moura and Leijen,
  [PLDI 2021](https://xnning.github.io/papers/perceus.pdf). Reference counting with reuse:
  Koka's purely functional red-black tree came within 10% of C++'s `std::map`. That benchmark
  is the best case for reuse — its trees are never shared, so every insertion reuses its cells —
  and the authors say it may favour reference counting and read all results as evidence of
  viability, not as absolute comparisons. Java used about 10 times the memory with default JVM
  settings; tuned, the gap fell to about 1.5 times, with Java slower. Disabling the reuse
  optimizations made `rbtree` more than 2 times slower, but `deriv` only slightly.
- **Counting Immutable Beans** — Ullrich and de Moura, [IFL 2019](https://arxiv.org/abs/1908.05647),
  the basis of Lean 4. Lean beat OCaml on `const_fold`, but the ablation does not credit borrow
  inference: turning it off made `const_fold` faster (0.90 of base), while turning reuse off made
  it 1.64 times slower. The paper puts the gap over OCaml mainly down to OCaml's collection time;
  borrow inference helped `binarytrees` and `deriv` instead. Lean's deallocation share excludes
  the time spent updating counts, which is inlined and cannot be measured, so it is not
  comparable with OCaml's collection share.
- **Mutable value semantics** — Racordon et al.,
  [JOT 2022](https://www.jot.fm/contents/issue_2022_02/article2.html). References are
  second-class; fixed-size values live on the stack and containers use copy-on-write. On 1,344
  generated programs Swift was fastest in most read-heavy programs, but the C++ side is a
  generated translation that copies a `std::vector` element on every read, and Swift was built
  with `-Ounchecked`. The "C++ wins above 90% writes" threshold is a median per write-ratio
  bucket; C++ is usually fastest in write-dominated programs.
- **Who adopted it:** Roc uses reference counting with Perceus; Nim uses ORC (counting with a
  cycle collector) by default since 2.0; Koka and Lean emit C with no collector.
- **Rust weighs, but not because of ownership.** In Nogueira et al.
  ([arXiv 2608.00661](https://arxiv.org/abs/2608.00661)) ownership and lifetimes are 2,165 of
  12,942 Rust compile-error labels (16.7%, baseline prompt), while type mismatches lead (43.4%) and
  trait errors are 6.1%. Translating to Rust, compile errors are 92.3% of failures
  ([RustRepoTrans](https://arxiv.org/abs/2411.13990), v6), and the borrow checker does not show up
  as a dominant cause. Rust's agentic cost penalty (1.16–1.57 times Python's where significant,
  [Tokenmaxxing](https://arxiv.org/abs/2607.22807)) is matched by Java, which has no ownership —
  so that figure says nothing about the borrow checker. What the evidence supports is narrower:
  a strict type system in a low-resource language costs type-mismatch errors.

## The hidden costs

- **Atomic counting.** Making every counting operation atomic cost 5% to 59% in Perceus, and
  forcing it on all values slowed Lean by 1.13 to 2.31 times. In Swift, counting took 32% of
  execution time on average even after the optimizer had removed up to 97% of the operations
  ([Biased Reference Counting](https://iacoma.cs.uiuc.edu/iacoma-papers/pact18.pdf), PACT 2018):
  a percentage of eliminated operations says little about cost. Swift pays for atomics in
  single-threaded programs because separate compilation stops it proving objects thread-private.
- **The "most operations are private" figure is weak.** Biased counting's 87–93% private share
  comes from client programs, five of seven single-threaded; the one concurrent client had about
  half of its operations shared.
- **Biased counting does not fit green threads.** An object stays biased to the thread that
  allocated it, with no ownership transfer, which migrating green threads break. Koka and Lean do
  what lotml needs instead: values start single-threaded and are marked shared, recursively and
  once, when they are handed to a new task — at a cost proportional to what the closure reaches.
- **Borrowing and reuse compete.** A borrowed parameter saves an increment and a decrement but
  can never be reused in place; Perceus has no borrowing on purpose, to stay garbage-free.
  Lean's borrow inference also breaks at partial application and closures, and can destroy tail
  calls. "Perceus-style reuse plus Lean-style borrow inference" is a trade-off to tune.
- **Reuse is fragile.** When trees are shared, as backtracking does, reuse stops firing and Lean
  slows down sharply; its developers needed a static analyzer and runtime counters to keep reuse
  working. A well-tuned tracing collector (MLton) beat or tied Lean on several benchmarks.
- **Inference stops at boundaries.** Swift had to add `borrowing`/`consuming` (SE-0377) and
  `~Copyable` (SE-0390) because the optimizer cannot change conventions fixed by the ABI.
- **Numbers that are not evidence:** the 95% of operations removed that Lobster announces are
  self-reported, with no methodology.

## Parameter conventions

Neither the original study nor the pilot spec says how a function changes a value that belongs
to its caller. In the pilot, Sonnet and Opus used the only documented path (methods with
`var self`); Haiku invented `var` parameters. Executing the programs settled what that does
([[python-leakage-pilot]]): in variant A, Haiku's ledger test expects the caller's dictionary to
change, and it fails under value semantics while passing under Python's reference semantics. In
variant B, Haiku's test only checks that nothing changes after a failed transfer, so it passes
either way.

The language needs explicit conventions, like Swift (`inout` with `&` at the call), Hylo and Mojo
(`mut`):

| convention | meaning | at the call |
| --- | --- | --- |
| default | read; the compiler borrows without copying | `f(x)` |
| `inout` | the function changes the caller's value | `f(&x)` — the mutation is visible |
| `sink` | the function takes ownership; the caller no longer uses it | `f(x)`, with the compiler forbidding later use |

What the implementation literature adds:

- **`inout` with copy-on-write needs care:** passed as an interior pointer, the callee cannot
  tell whether it lands in shared storage, so the caller copies non-unique buffers first.
- **Exclusivity is checked syntactically:** the same access path may not appear twice among a
  call's `inout` arguments, and indices that cannot be proved distinct are rejected.
- **A copy into a `var` is a real copy:** alias-for-copy works only for immutable bindings, so a
  callee that copies a parameter into a `var` to mutate it pays for that copy.
- **`sink` is a contract more than an optimization:** moves are inferred for temporaries and last
  uses; the keyword makes ownership transfer part of the API.
- **Closures must capture by copy:** Swift's by-reference captures reintroduce aliasing and are
  checked only at runtime; Swiftlet captures by copy to keep the guarantee static.

Derived requirements: the marker at the call makes the mutation visible; `var self` and `inout`
should be the same idea with one word; and the [[semantic-compiler]] warns when a copied parameter
is changed and discarded, suggesting `inout` — the exact error the pilot showed.

## Cycles

Without stored references, cycles cannot form. Languages that allow stored mutable references pay
for it: Swift requires `weak`/`unowned`, Nim runs a cycle collector (ORC), Lobster reports cycles
at program exit. A mutable cell shared between threads needs more than atomic counts — read-and-
increment races with write-and-decrement — which is another reason not to store references.
Graphs use the arena-and-index idiom, like the adjacency-dictionary graph in the paired corpus.

## Performance target

- **"1–2× C on numeric benchmarks" is plausible for numeric code:** Nim sits at 1.0–1.3× on most
  benchmarks.
- **Allocation-heavy code is where counting loses:** in the Benchmarks Game, Swift 6.0.3 ranges
  from 1.03× C (pidigits) to 10.5× (binary-trees) and 21.7× (regex-redux); Perceus's authors expect
  a tracing collector to win when reuse does not fire and many short-lived objects are allocated.
- **"10–100× faster than CPython"** is conservative: CPython 3.13 is 33× to 486× behind C++ on
  benchmarks that do not call a C library.
- **Checked arithmetic is not free:** the MVS benchmarks disabled overflow and bounds checks, so
  they do not show what lotml's trap-on-overflow costs on the C target; on the Python target it is
  measured in [[transpilation-strategy]].

The project's own benchmark must include allocation-heavy and sharing-heavy programs, not just
numeric ones, and report them separately.

## Recommendations

1. Keep value semantics with reference counting and Perceus-style reuse; add borrow inference
   only where measurement shows it pays, since it competes with reuse.
2. Explicit parameter conventions, with a visible marker at the call, already in v1 — the Python
   target needs them too ([[transpilation-strategy]]).
3. Non-atomic counting by default; values handed to a task are marked shared once, as Koka and
   Lean do — not biased counting, which assumes objects stay on their thread.
4. No stored references; closures capture by copy; arena and indices in the standard library.
