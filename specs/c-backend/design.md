# C backend — design

## What changes

Serves R1.1, R1.2, R2.3, R3.1, R5.1.

A new crate, `compiler/crates/lotml-c`, compiles a checked module to one C file, and the `lotml`
CLI's `run`, `test` and `build` take `--target c` (`python` stays the default). The architecture is
adr:0014-c-target-as-monomorphic-c-over-a-counting-runtime; this file is how its parts meet.

```
lotml_syntax::Module + lotml_check::Checked
  └─ lower.rs   typed intermediate form: monomorphic, every intermediate value named, places explicit
      └─ own.rs  counts: moves at last use, increments before, decrements where a value dies; reuse
          └─ emit.rs  C with #line, plus one descriptor and helper set per concrete type
runtime/lotml.h + runtime/lotml.c   included into the program: one translation unit
driver.rs   finds the C compiler, builds, runs
```

The intermediate form is the unit the passes share: functions of statements over typed locals,
structured control flow (`if`, `loop` with `break`/`continue`, `switch` on a variant's tag,
`return`), and operands that are locals or constants. Lowering takes each expression's type from
`Checked::types` with the instantiation's type arguments substituted, and resolves every call to a
function, a method of an `impl`, a runtime builtin, a constructor or a closure call before the C is
written, so no name is looked up at run time.

## Data

| lotml | in C |
| --- | --- |
| `int`, sized integers, `f64`, `f32`, `bool` | `int64_t` … `uint64_t`, `double`, `float`, `bool` |
| `None` (unit) | no value; a unit-typed local is not declared |
| `(A, B)` | a struct of its fields, by value |
| `T?` | `struct { bool some; T value; }`, by value |
| `T ! E` | `struct { bool ok; union { T value; E error; }; }`, by value |
| `str` | `lt_str *`: count, byte length, code point count, ASCII flag, UTF-8 bytes |
| `[T]`, `{K: V}`, `{T}`, `Heap[T]` | `lt_list *`, `lt_dict *`, `lt_set *`, `lt_heap *`, elements stored inline at their C size |
| a record | a cell: header, then its fields as a C struct |
| a sum type | a cell: header, tag, then the variant's fields; a variant without fields is a static cell |
| a closure | a cell: header, function pointer, then the captured values |
| `dyn T` | `struct { void *cell; const vtable *v; }`, the value boxed in a cell |

Every cell starts with `int32_t count`: positive while private to a task, negative once marked
shared (R3.3), zero for a static cell that is never counted or freed. A collection's functions in
the runtime take a descriptor of the element type — size, increment, decrement, equality, order,
hash, `repr`, `str`, mark shared — which `emit.rs` writes once per concrete type.

## Counting

Serves R3.1, R3.2, R3.4, R3.5.

- A local of a heap type holds one count. Every parameter except `inout` is owned: the caller
  increments an argument it still uses afterwards and moves it otherwise; the callee decrements
  what it has not moved on by the time it returns.
- Liveness is computed per function over the intermediate form, loops to a fixed point. A use that
  stores a value (in a binding, a container, a cell, a capture, a return, an owned argument) moves
  the operand at its last use and increments it before; a use that only reads it (an index, a
  length, a comparison, a format) does neither. A value is decremented right after its last use,
  or at the start of a branch that never uses it.
- Changing a value in place — `append`, `xs[i] = v`, `p.x = v`, `inout` — first makes it unique: a
  count above one copies the cell (shallowly, incrementing the elements) and decrements the
  original. A shared value is never unique.
- Reuse: in a `match` arm whose subject dies at the arm's start, the arm's bound fields are moved
  out when the subject is unique and incremented when not; a constructor of the same cell size in
  that arm takes the unique subject's memory instead of allocating.
- A test build counts cells allocated and freed and reports the difference at exit (R3.6).

## Text and order, as CPython

Serves R1.3.

`print` and `str` use each type's `str`, which is `repr` except for `str` itself; a container shows
its elements by `repr`. Float `repr` is the shortest of `%.1e` … `%.17e` that reads back exactly,
laid out as CPython does (`1e+16`, `1.5e-05`, `2.0`). String `repr` picks single quotes unless the
text holds a single quote and no double one, and escapes as CPython does for ASCII control
characters; other characters print as they are. Records and variants print as their dataclass:
`Point(x=1.0, y=2.5)`, `Num(_0=3)`, `Empty`; `Ok(value=…)`, `Err(error=…)`. Sets keep CPython's
open-addressing table — the same initial size, probe sequence, growth and hash (`int` modulo
2^61 − 1, `float` per `_Py_HashDouble`, `str` by SipHash-1-3 over its PEP 393 width with the key
`PYTHONHASHSEED=0` gives, tuples per xxHash) — so that iteration order matches; dicts keep insertion
order. The parity suite runs the Python target with `PYTHONHASHSEED=0`.

## Running and tests

Serves R1.4, R2.2, R4.1, R5.1, R5.2.

`driver.rs` writes `<name>.c` with the runtime included and builds it with `-std=c11 -O2` (gcc,
clang) or `/std:c11 /O2` (`cl`), plus `-lm` and threads. A panic prints `panic: <kind>: <message>`
and `  File "<file>.lotml", line <n>, in <function>` to standard error and exits with 101, as the
Python target's `main` does; each panic site passes its line, and the runtime keeps a stack of the
active functions' names for the `in` part. `lotml test --target c` compiles the module's `test`
blocks into a `main` that runs each one under `setjmp`, a failed `assert` or panic jumping back to
report it, and prints the report `lotml test` prints for the Python target. `parallel` runs each
task on a thread (`pthread` or Win32 threads, at most 256 at once) after marking the captured values
shared.

## Risks

- CPython's set order depends on its exact table algorithm; a difference shows only for programs
  that print or iterate a set without sorting it. The parity suite is what finds it.
- Unicode case mapping and `isalpha` beyond Latin-1 are a table the runtime does not carry; such
  text is a known divergence, recorded as a note when the suite meets it.
