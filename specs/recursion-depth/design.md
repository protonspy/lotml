# Recursion depth — design

## The limit: 1,000, counted by the program

CPython's own limit counts every Python frame: the runtime's helpers and the module's frame are
counted with the program's, so where a lotml program stops depends on how the Python backend
writes it. A native program has no limit at all, only a stack. For both targets to stop at the
same call, the program counts its own calls and CPython's limit is moved out of the way (R2.2).

1,000 is CPython's default, the limit a program written by a Python-trained model already
respects; raising it later breaks no program that runs today.

## Which calls count

A call can only nest without bound through a cycle: a function in a strongly connected component
of the call graph, or one whose value is taken, since a call through a value can reach anything
(R1.1, R1.4). The lowering computes the set once, over the IR every backend reads
(adr:0020-one-ir-between-the-checker-and-every-backend), and marks those functions; the others,
most of a typical program, pay nothing. That keeps R3.1 within reach: the counted functions'
bodies get one increment, one compare and one decrement.

## Where the count lives

- **IR.** A marked function's body begins with `Enter` and every return leaves through `Leave`,
  inserted by a pass after lowering, so both backends emit the same points (R1.3). A panic ends
  the program, so it needs no `Leave`.
- **Python target.** `Enter` and `Leave` become calls of the runtime that keep a per-thread counter
  (`threading.local`, as `parallel` runs tasks on threads); the runtime sets
  `sys.setrecursionlimit` to well over the limit times the frames a lotml call takes (R2.2).
- **LLVM target.** The counter is a thread-local `int32_t` of the runtime; `Enter` is emitted
  inline as an increment and a compare that calls `lt_recursion_error` past the limit, and `Leave`
  as a decrement.

## Room on the stack

The runtime's `main` and its `parallel` workers run lotml code on threads created with a reserved
stack (R2.1), sized from the largest frame `clang` reports for the program's functions
(`-fstack-usage` gives each function's frame) times the limit, with a margin for the uncounted
calls between counted ones and a floor of 16 MiB. A reservation costs address space, not memory:
the pages are committed only as they are touched.

## What is ruled out

- **CPython's limit alone**: it counts frames lotml does not control, so the two targets would
  stop at different calls.
- **A guard page caught natively**: catching the overflow differs on every system and stops in the
  middle of a frame, with no line to name.
- **Counting every call**: correct but paid by every function, against R3.1.
