# Colorless concurrency

The original study proposes concurrency without marking functions `async` or requiring `await`,
with tasks on green threads, as in Go and BAML, to eliminate the "forgot the `await`" error and
keep `async` from spreading through the code. The idea holds; the evidence shows where colorless
runtimes leak and what the idea demands of the [[memory-model]].

## Precedents

- **The argument:** Bob Nystrom, *What Color is Your Function?* (2015) — Go "eliminated the
  distinction between synchronous and asynchronous code".
- **Go:** goroutines preemptible since 1.14, at the cost of more `EINTR` errors on Unix.
- **Java virtual threads (JDK 21, JEP 444):** a virtual thread gets pinned to its OS thread inside
  `synchronized` and in native calls; JDK 24 (JEP 491) removed the `synchronized` case, and class
  initialization still pins.
- **Zig:** 0.15 announced that "there will not be async/await keywords"; I/O goes through an `Io`
  interface received as a parameter, like the allocator. In 0.16 (April 2026) the threaded
  implementation is complete and the evented one is still experimental. Passing `Io` as a
  parameter turns the "color" into a parameter — see [[type-system]].
- **OCaml 5:** untyped effects, with about 1% average cost for code that does not use them.
- **BAML:** green threads and "colorless concurrency like Go", according to its README.

## Where it leaks

Colorless runtimes leak at the boundary with code that blocks without warning: native calls and
FFI (Java's virtual threads), C extensions that do not yield (gevent in Python), signals (Go's
`EINTR`). lotml will call Python and C, so it needs a mechanism to hand blocking FFI calls to
dedicated threads, as Go does with syscalls.

## What it demands of memory

Green threads that migrate between OS threads would force reference counting to be atomic, and
atomic counting cost 5–59% in Perceus and 1.13–2.31× in Lean ([[memory-model]]). Biased counting
does not rescue migrating tasks: it biases each object to the OS thread that allocated it and has
no ownership transfer, so a task that migrates hits the atomic path for everything it allocated
before moving. What works is what Koka and Lean do, and what the original study's rule implies:
values start thread-local, and a value handed to another task is marked shared once — or copied
or moved — at the spawn, so counting inside a task stays non-atomic. Structured concurrency
(scoped task groups) keeps that boundary explicit; the price is a marking pass proportional to
what the spawned closure reaches.

## On the Python target

- **gevent:** cooperative greenlets on a single thread; blocking C extensions do not yield.
- **Free-threaded CPython:** supported since 3.14 (PEP 779), costing about 1% to 10% on
  single-threaded code depending on the platform; 3.15 ships on 2026-10-09 still not as the
  default.

The Python target ([[transpilation-strategy]]) does not reproduce green threads faithfully; since
concurrency is v2, mapping tasks to threads and documenting the differences is enough.

## What phase 2 built

On the Python target, concurrency is one prelude function, `parallel(tasks)`: each task, a
function with no parameters, runs on a thread of its own, and the caller waits for them all and
gets their results in order — structured, so no task outlives the call that started it. No
function is marked `async` and nothing is awaited; Python's `async` and `await` are diagnosed
(E0112) and the fix removes them, and calls given where tasks are wanted, `parallel(f(a), f(b))`,
are diagnosed with the fix that wraps each in a lambda. A call that blocks — into Python, say —
holds up only its own thread, which is the dedicated thread the leaks above ask for; a task that
panics stops the program once the others have finished. Values need no marking on this target:
a task reaches only immutable values, which it can share, and copies, since a lambda captures a
copy of each local it uses when it is made. Building it found that the backend did not: Python
reads a closure's variable when it is called, so lambdas made in a loop all saw its last value;
each captured local is now a default bound when the lambda is made. With the GIL the threads
interleave rather than run in parallel; free-threaded CPython runs them at once.

## Unmeasured hypothesis

No source found measures how often "forgot the `await`" or spreading `async` happens in
LLM-generated code. The original study's argument is plausible, but it is a hypothesis: it enters
the [[evaluation-harness]] as a question, not as a fact.
