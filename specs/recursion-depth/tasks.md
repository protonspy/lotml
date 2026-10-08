# Recursion depth — tasks

## 1 · The count

- [x] 1.1 (TDD) Mark, in a pass over the IR after `mono`, the functions in a cycle of the call graph, used as a value, or methods of a trait used through `dyn` — R1.1, R1.4, R2.3
- [ ] 1.2 (Unit) Insert `Enter` at the start of each marked function and `Leave` on each of its returns — R1.3
  _Depends 1.1_
- [ ] 1.3 (TDD) Count on the Python target with a per-thread counter and its own `RecursionError` past 1,000, checked before it increases, carried into `parallel` tasks and restored where a `test` block or a task ends in a panic — R1.2, R1.5, R1.6
  _Depends 1.2_
- [ ] 1.4 (TDD) Count on the LLVM target with a thread-local counter emitted inline, checked before it increases, carried into `parallel` tasks, panicking through the runtime past 1,000 and restoring the count at the `test` runner's and the workers' jump points — R1.2, R1.5, R1.6
  _Depends 1.2_

## 2 · Room and cost

- [ ] 2.1 (Unit) Run `main`, the `test` runner and the `parallel` workers natively on threads reserving 64 MiB, with the reservation flag on Windows, keeping today's exit codes and stopping with a named panic when `main`'s thread cannot be made — R2.1, R2.4
  _Depends 1.4_
- [ ] 2.2 (Unit) Run `main` and the tests on the Python target on a thread with a 64 MiB stack and CPython's limit at 20,000, set by `lotml run` and `lotml test` only, never on a host's import — R2.2
  _Depends 1.3_
- [ ] 2.3 (TDD) Add parity programs that recurse to 1,000 and past it directly, mutually, through a lambda and through `dyn`, through `parallel`, a `test` block that panics deep followed by one that recurses, a function with many locals, and an exported function recursing from its host, on both targets, at both levels and on CPython 3.11 and 3.14 — R1.1, R1.2, R1.3, R1.4, R1.5, R1.6, R2.1, R2.2, R2.3
  _Depends 2.1, 2.2_
- [ ] 2.4 (Unit) Measure the benchmarks with and without the count, fastest of seven runs, in `harness/results/benchmarks-llvm.md`, and record a ceiling note if one passes 5% — R3.1
  _Depends 1.4_
