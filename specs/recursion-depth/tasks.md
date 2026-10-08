# Recursion depth — tasks

## 1 · The count

- [ ] 1.1 (Unit) Mark, in a pass over the lowered IR, the functions in a cycle of the call graph or used as a value — R1.1, R1.4
- [ ] 1.2 (Unit) Insert `Enter` at the start of each marked function and `Leave` on each of its returns — R1.3
  _Depends 1.1_
- [ ] 1.3 (TDD) Count on the Python target with a per-thread counter, panic past 1,000 with `RecursionError`, and raise CPython's limit above it — R1.2, R2.2
  _Depends 1.2_
- [ ] 1.4 (TDD) Count on the LLVM target with a thread-local counter emitted inline, panicking through the runtime past 1,000 — R1.2
  _Depends 1.2_

## 2 · Room and cost

- [ ] 2.1 (Unit) Run `main` and the `parallel` workers on threads whose stack is sized from the program's largest frame, with a floor of 16 MiB — R2.1
  _Depends 1.4_
- [ ] 2.2 (Unit) Add parity programs that recurse to 1,000 and past it, directly, mutually and through a lambda, on both targets and at both levels — R1.1, R1.2, R1.3, R1.4, R2.1
  _Depends 1.3, 2.1_
- [ ] 2.3 (Unit) Measure the benchmarks with and without the count and record it in `harness/results/benchmarks-llvm.md` — R3.1
  _Depends 1.4_
