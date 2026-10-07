# LLVM parity — design

## What changes

Serves R1.1, R1.2, R1.3, R3.1.

`lotml-llvm`'s emitter grows to every statement and expression of the counted IR; the runtime
does the rest, as it did for the C target. Every heap value, collection operation, text rule
and hash already lives in the runtime crate (plan task 1.1), so this spec is mostly calls into
it, and the Python target is the reference: a program whose LLVM output differs from it is an
emitter bug.

## Data

Serves R1.3, R3.1.

The runtime reads cells, descriptors and closures by their C layout, so the LLVM types the
emitter writes must be that layout as `clang` lays it out for the same triple. The values are
those of `specs/c-backend/design.md`'s table:

| LotML | in LLVM |
| --- | --- |
| tuples, `T?`, `T ! E` | literal struct types by value: `{ i1, T }`, `{ i1, [N x i8] }` holding the larger of `T` and `E` with its alignment |
| `str`, collections, records, variants, closures | `ptr` to a cell whose header is `i32 count`, then the fields as the C struct has them |
| a type descriptor | a constant global of the runtime's descriptor struct, one per concrete type, with pointers to emitted `inc`/`dec`/`eq`/`hash`/`repr` functions |
| `dyn T` | `{ ptr, ptr }`: the cell and a constant vtable global |

The layout of each runtime struct the emitter mirrors is asserted by a test that compiles a C file
printing `sizeof` and `offsetof` with the same `clang` and compares them with the emitter's. Layout
is half of the contract; the other half is how a value is passed. Tuples, optionals and results
travel by value only between functions the emitter writes; into and out of the runtime they go
through a pointer (the rule `specs/llvm-backend/` sets for every call into the runtime), because
LLVM leaves a by-value aggregate's C ABI to the frontend.

Counting is written in IR, not called. `lt_inc`, `lt_dec`, `lt_unique` and the other `static
inline` helpers of `lotml.h` are not symbols the runtime exports (n-0079); the emitter writes
each as the same few instructions on the cell's `i32` count, which `-O2` inlines as the C
compiler does, rather than paying a call per `Inc` and `Dec` on the path the benchmarks time.

## Running and tests

Serves R1.4, R2.1, R2.2, R3.2, R3.3, R4.1.

- `test` blocks: the C target writes `setjmp` into the generated `main`. LLVM IR would need
  `returns_twice` handling for that, so the runtime gains a C function that runs one test
  through a function pointer under its own `setjmp` and reports it; the emitter writes a `main`
  calling it once per block.
- Debug lines: a `DICompileUnit` for the `.lot` file, a `DISubprogram` per function and a
  `!dbg` location per instruction from the statement's span — the counterpart of the C target's
  `#line`. Written at `-O0`, the level `run` uses.
- `parallel` and shared marking are runtime calls, as on the C target. A `c.<library>` call is a
  direct `call` to a declared symbol, and the library goes on `clang`'s command line.
- The parity experiment (`harness/lotml_harness/experiments/parity.py`) compares the LLVM target,
  built at `-O2`, the level `build` ships, with the Python target; `benchmarks.py` builds each
  benchmark with `--target llvm` and times it against its hand-written C.
- Debug lines are checked by reading the line table `llvm-dwarfdump --debug-line` prints, not by
  driving a debugger in CI.

## Risks

- A layout mismatch between emitter and runtime corrupts memory silently. The `sizeof`/`offsetof`
  test and the leak report in test builds are the guards.
- The C target's per-type helper functions (`inc`, `dec`, `eq`, …) are written by `emit.rs` in
  C today. The LLVM emitter writes them again in IR; if that grows past a few hundred lines, the
  alternative is generating them into a C file compiled beside the program, recorded as a note
  before choosing.
