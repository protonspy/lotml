# Recursion depth — design

## The limit: 1,000, counted by the program

CPython's own limit counts every Python frame: the runtime's helpers and the module's frame are
counted with the program's, so where a lotml program stops depends on how the Python backend
writes it. A native program has no limit at all, only a stack. For both targets to stop at the
same call, the program counts its own calls (R1.1), raises its own `RecursionError` with an exact
message (R1.2), and CPython's limit is moved out of the way (R2.2).

1,000 is CPython's default, the limit a program written by a Python-trained model already
respects. Raising it later breaks no program that runs today; lowering it would, which is why it
is a number in this spec rather than a setting.

## Which calls count

A call can only nest without bound through a cycle. The lowering marks, once, over the IR every
backend reads after `mono` (adr:0020-one-ir-between-the-checker-and-every-backend):

- the functions of every strongly connected component of the call graph with a cycle in it;
- every function whose value is taken, since a call through a value can reach anything;
- every method of a trait used through `dyn`, since a dynamic call has no edge in the graph.

The others, most of a typical program, pay nothing. A call into Python or C is not lotml's and is
not counted; a native library's exported function, called by its host, counts on the host's
thread like any other (R2.3).

## Where the count lives

- **IR.** A marked function's body begins with `Enter` and every return leaves through `Leave`,
  inserted by a pass after lowering, so both backends emit the same points (R1.3).
- **Python target.** `Enter` and `Leave` become calls of the runtime on a per-thread counter
  (`threading.local`). Past the limit the runtime raises its own panic of kind `RecursionError`
  with the exact message, never CPython's builtin, whose message varies.
- **LLVM target.** The counter is a thread-local `int32_t` of the runtime. `Enter` is emitted
  inline as a compare that calls `lt_recursion_error` at the limit, then an increment, and `Leave`
  as a decrement; the check comes first, so a caught `RecursionError` leaves no count behind.
- **`parallel`.** A task carries its spawner's count, and its worker starts from it (R1.6): a
  thread-local alone would start every task at 0, and a recursion through `parallel` would meet
  no limit, only the system's threads and address space.
- **A panic caught.** A panic does not unwind through `Leave`: the native runtime `longjmp`s to the
  `test` runner or to a `parallel` worker, and the Python runtime catches it in the same two
  places. Each saves the count before the block or task and restores it after a panic (R1.5), or
  the next block on that thread would start deep, or past the limit.

## Room on the stack

- **Native.** The runtime runs `main`, the `test` runner and the `parallel` workers on threads it
  creates with a 64 MiB stack: `pthread_attr_setstacksize` on Linux and macOS, and on Windows
  `_beginthreadex` with `STACK_SIZE_PARAM_IS_A_RESERVATION`, without which the size is committed
  rather than reserved and 256 workers would commit 16 GiB. The process's own `main` starts that
  thread, joins it, flushes the output and exits with its status, so the exit codes of today
  (0, 1, 101) do not change. 64 MiB is 64 KiB for each of the 1,000 counted calls and the
  uncounted calls between them: lotml keeps lists, strings and records on the heap, so a frame is
  its scalars and pointers, a few hundred bytes at `-O0`. The parity programs of task 2.2 hold it
  to that at both levels.
- **Python.** `threading.stack_size(64 MiB)` and the limit of 20,000 are set by the runtime's
  entry for `lotml run` and `lotml test`, never on importing a module, so a Python host that
  imports a compiled module keeps its own limit (R2.2). They are set before the runtime starts the
  thread `main` and the tests run on; the pool `parallel` uses creates its threads after it. With the stack in place,
  CPython's limit of 20,000 frames leaves room for twenty frames per lotml call, more than the
  runtime's wrappers take, and C-level recursion — the `repr` of nested data, which 3.11 guards
  with the same limit — cannot reach the end of a 64 MiB stack before the limit.

If the thread for `main` cannot be created, the program says so and stops (R2.4); a `parallel`
worker that cannot be created keeps today's behavior, the task running inline.

## What is left out

Recursion over data rather than over calls: dropping, sharing, comparing, hashing or printing a
value of a recursive type, `Node(Node(…))`, runs the runtime's generated functions once per level,
and no lotml call is made. A value deep enough still exhausts a native stack, where CPython raises
its own error. Counting those functions, or making them iterate over a work list, is a change of
its own (n-0103); this spec bounds the calls a program makes.

## What is ruled out

- **CPython's limit alone**: it counts frames lotml does not control, so the two targets would
  stop at different calls.
- **A linker option for the stack**: `/STACK` sizes only the main thread on Windows, and Linux has
  no link-time setting; `setrlimit` applies only to a process started after it. A thread the
  runtime creates is the one shape that works on every system.
- **Sizing the stack from `-fstack-usage`**: it needs a second compile and its report is
  unverified for COFF; a fixed reservation costs address space only.
- **A guard page caught natively**: catching the overflow differs on every system and stops in the
  middle of a frame, with no line to name.
- **Counting every call**: correct but paid by every function, against R3.1. If the count still
  costs more than R3.1 allows, the benchmarks' numbers are recorded as a ceiling note naming the
  cost, rather than the requirement silently missed.
