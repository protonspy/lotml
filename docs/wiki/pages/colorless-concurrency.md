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

On the native target, `parallel` runs each task on an operating-system thread, at most 256 at once
(`LT_TASK_THREADS` in `lotml.c`), each reserving 64 MiB of stack, and returns the results in
order. What the tasks capture is marked shared first: a shared cell's count turns negative and only
then changes atomically, so cells no task reaches keep plain counts
(adr:0016-c-target-as-monomorphic-c-over-a-counting-runtime). A blocking C call holds up only its
own task's thread, and a native program loads no CPython
(adr:0025-two-targets-python-for-run-llvm-for-build).

## The forgotten-`await` hypothesis, measured

No source found measures how often "forgot the `await`" or spreading `async` happens in
LLM-generated code, so it entered the [[evaluation-harness]] as a question
(`harness/results/awaits.md`). Four models wrote 30 concurrency tasks twice: with asyncio, given
helper coroutines, and in lotml, given helper functions and `parallel`. The hypothesis was not
borne out. In 120 asyncio answers there was one forgotten `await` (Llama made coroutines it never
awaited, and still returned the right values) and no colour mistake; every Python answer ran the
calls concurrently, and 117 were right. Writing lotml, the frontier models made no concurrency
mistake either, but the open ones misused the tasks 26 times: Qwen 23 of 30 times, writing each
task as `lambda k: f(k)` inside `[… for k in keys]`, which is the habit `map` builds. So the
colorless form traded a mistake models rarely make for one the small ones make often, before any
feedback. The compiler now offers the fix for that habit (drop the parameter the loop binds). What
the colorless form keeps is what the paper argued from: no function's signature changes because it
starts waiting.
