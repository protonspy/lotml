---
status: accepted
---

# 0019 · Proceed to phase 4 past the failed phase 3 gate

## Context

The phase 3 gate asks for the same suite green on the Python and C targets, and the C target
within 2× C on numeric benchmarks ([[evaluation-harness]], task 4.4 of `plans/lotml-roadmap.md`).
Its two runs split:

- **Parity passes** — 509 programs of the corpus run their `test` blocks on both targets and
  report the same rows, none differing, none refused (`harness/results/parity.md`).
- **Numeric performance fails** — built by `cl.exe` with the backend driver's options, fastest of
  seven runs (`harness/results/benchmarks.md`): fib 1.13×, mandelbrot 0.89× and sieve 1.81× are
  within 2×, collatz takes 2.27× and matmul 7.48×. The lotml programs check every integer
  operation and index; the C ones do not. Neither outlier has been profiled.

adr:0011-proceed-to-phase-2-past-the-failed-phase-1-gate said the later gates keep their force,
naming phase 3's parity as an executable check; parity is the half that passed. What fails is the
performance half, and phase 4 is the native backend that is judged by the same 2× target. At the
same time the compiler is being restructured so every backend reads one IR
(adr:0020-one-ir-between-the-checker-and-every-backend, `plans/ir-architecture.md`): the lowering
where a cost like matmul's would live moves out of the C backend into the IR, where both native
backends share it.

## Decision

Record the phase 3 gate as passed on parity and failed on numeric performance, and proceed to
phase 4 as `plans/ir-architecture.md` scopes it. The result and the criterion stay as they are;
this record overrides only the consequence that the next phase does not start. Not taken: fixing
collatz and matmul on the C target before the restructuring, which would tune a lowering about to
move into the shared IR and delay the native backend the project now prioritises; revising the
criterion after seeing the result — counting only programs without nested lists, or comparing
against C with the same checks; and stopping the roadmap at phase 3.

## Consequences

- The numeric gap stays an open, measured risk. The benchmarks are run again on each native
  target as it reaches parity, recorded as new runs beside this one, never in its place.
- 2× C on numeric code remains the target both native backends are measured against, and the
  first place a fix is looked for is the shared lowering, where it serves both.
- `plans/lotml-roadmap.md` cannot meet its "within 2× C" done condition until a run shows it; the
  roadmap stays open on that line rather than being closed by this record.
- Two of the roadmap's four gates have now been overridden. A third override is evidence that the gates are set
  wrong or followed loosely, and is worth asking about as such.
