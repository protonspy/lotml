---
autonomy: auto
ci: wait
branch: docs/recursion-depth-spec
delivery: in-progress
---

# Recursion depth — requirements

## Purpose

A program that recurses too deeply stops the same way on both targets. Today the Python target
stops at CPython's limit of 1,000 frames, which counts the runtime's frames as well as the
program's, with `RecursionError`; a native program runs until the operating system's stack is
gone, 1 MiB on Windows and 8 MiB by default on Linux, and dies with an access violation that no
`lotml` report names (n-0095). A program that runs on one target and crashes on the other is the
gap the parity suite exists to close (plans/target-parity-assurance.md 4.1,
docs/wiki/pages/target-parity.md).

## Requirements

### 1 · One limit, counted in lotml's calls

- **R1.1** The compiled program shall count, on each thread, the calls in progress of every lotml function that can recurse (one in a cycle of the program's call graph, or one used as a value) on both targets, and no other frame: not the runtime's, not CPython's.
- **R1.2** If a call would make the count on its thread greater than 1,000, then the compiled program shall stop with a panic of kind `RecursionError` and the message `maximum recursion depth exceeded`, naming the call's line and function as every panic does.
- **R1.3** When a counted call returns with a value, with an error or by `fail`, the compiled program shall decrease the count.
- **R1.4** Where a function is called through a value (a lambda, a function passed as an argument, a method through `dyn`), the compiled program shall count the call as it counts a direct one.

### 2 · Room for the limit

- **R2.1** The native program shall run `main`, and every task `parallel` starts, on a thread whose stack holds 1,000 counted calls of the program's largest frame and the calls between them, at `-O0` and at `-O2`, so that the limit, and never the stack, stops a recursion.
- **R2.2** The Python target shall set CPython's own limit high enough that the panic of R1.2 always comes first.

### 3 · What it costs

- **R3.1** When a benchmark of `harness/benchmarks/` is built with the count, the compiled program shall take no more than 5% longer than without it.
