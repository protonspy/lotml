---
autonomy: auto
ci: wait
branch: feat/recursion-depth
delivery: merged
pr: 50
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

- **R1.1** The compiled program shall count, on each thread, the calls in progress of every lotml function that can recurse (one in a cycle of the program's call graph, a method of a trait used through `dyn`, or one used as a value) on both targets, and no other frame: not the runtime's, not CPython's, not a foreign function's.
- **R1.2** If a call would make the count on its thread greater than 1,000, then the compiled program shall stop with a panic of kind `RecursionError` and the message `maximum recursion depth exceeded`, raised by the program's own check before the count increases and naming the call's line and function as every panic does.
- **R1.3** When a counted call returns with a value, with an error or by `fail`, the compiled program shall decrease the count.
- **R1.4** Where a function is called through a value (a lambda, a function passed as an argument, a method through `dyn`), the compiled program shall count the call as it counts a direct one.
- **R1.5** When a `test` block or a `parallel` task ends in a panic, the compiled program shall give its thread back the count it had when the block or task began.
- **R1.6** When a `parallel` task starts, the compiled program shall start its count at the count of the thread that started it, so that a recursion through `parallel` meets the same limit.

### 2 · Room for the limit

- **R2.1** The native program shall run `main`, the `test` blocks and every task `parallel` starts on threads that reserve 64 MiB of stack, so that the limit, and never the stack, stops a recursion.
- **R2.2** When `lotml run` or `lotml test` runs a program on the Python target, the runtime shall run `main` and the `test` blocks on a thread with a 64 MiB stack, with CPython's own recursion limit set to 20,000, so that the panic of R1.2 always comes first and CPython's C-level recursion has room on 3.11 as on 3.14, leaving the limit of a Python host that imports a compiled module alone.
- **R2.3** Where an exported function of a native library is called by its host, the compiled program shall count its calls on the host's thread as R1.1 does, the stack being the host's.
- **R2.4** If the thread for `main` cannot be created, then the native program shall stop with a panic naming it, rather than run on the system's smaller stack.

### 3 · What it costs

- **R3.1** When a benchmark of `harness/benchmarks/` is built for the LLVM target with the count, the compiled program shall take, in its fastest of seven runs, no more than 5% longer than without it.
