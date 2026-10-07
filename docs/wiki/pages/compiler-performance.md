# Compiler performance

This page covers where lotml spends time when it checks, builds and runs a program, and what the
projects in [[prior-art-compilers]] do about the same costs. The under-100 ms check on every edit
is a requirement ([[semantic-compiler]], [[transpilation-strategy]]). Native speed is measured
against hand-written C (adr:0025-two-targets-python-for-run-llvm-for-build).

## Building: the runtime dominates

`lotml build` hands `clang` the program's `.ll` and the runtime's `lotml.c` together, on every
build and in every LLVM test (`compiler/crates/lotml-llvm/src/driver.rs`). The study timed
`clang` on lotml's runtime on one Windows machine:

| step | `-O0 -g` | `-O2` |
|---|---|---|
| compiling `lotml.c` | ~250 ms | ~915 ms |
| a small program's `.ll` | 55–80 ms | |
| linking | ~95 ms | |

The runtime is therefore about 60% of a small debug build and 80% of a release build. mun builds
its runtime once, and a library it generates contains only user code. plix keys its object cache
on a fingerprint of the toolchain (`build.rs`).

Program and runtime are already separate translation units, so caching the runtime's object
loses no inlining. The cache key must cover:

- the runtime's sources;
- `clang`'s version;
- the optimization, sanitizer and shared-library flags;
- the target.

That cache is built (plans/build-and-check-speed.md): the runtime's object is kept per user,
keyed by all four, and a second build takes it from there. On the same machine a small build
went from 0.37 s to 0.12 s at `-O0` and from 0.92 s to 0.08 s at `-O2`
(`harness/results/build-speed.md`); [[transpilation-strategy]] says how an entry is kept sound.

This also answers the Cranelift question. A faster code generator could replace only the `.ll`
step, which is the smaller one. plix's native code still loses to CPython on calls because of
its runtime design: boxed arguments, floats on the heap, and an error-flag check after every
operation. adr:0021-compiler-in-rust-with-llvm-as-its-native-code-generator stands.

## Checking: salsa at the right grain

`lotml-db` has two coarse spots:

- the `checked` query checks a whole file, and its results carry absolute spans, so one keystroke
  re-checks every function;
- `interfaces()` is a plain function rather than a tracked query, so every `.lotmli` is parsed
  again on each check.

ty runs on the same salsa 0.28 at a finer grain. It has a type query per scope and per definition,
and a query whose result comes out unchanged does not re-run the queries that depend on it
(`ty_python_semantic/src/types/infer.rs`). Files are inputs with a durability, so library files
are assumed not to change (`ruff_db/src/files.rs`). mun splits a function's signature from its
body, so editing a body leaves every caller's check untouched.

lotml has an advantage over both: signatures are fully annotated, so a function can be checked
without its callers' bodies. The split is per-function check queries with spans relative to their
item, and interfaces as tracked inputs with high durability.

## Native hot paths

- **Reference counting is an out-of-line call.** `lt_inc` is inline in `lotml.h`, but the emitted
  IR calls the extern copy in `lotml.c`, and the two units are not optimized together. pon's
  lesson is the same: one helper call per operation is what its code pays. The fix is to emit the
  fast path in the IR itself. Link-time optimization would also do it, but needs `lld`, which
  touches adr:0021.
- **Constant collection literals allocate every time.** A list literal is `lt_list_new` plus one
  push per element. The runtime already has static string cells with a count of 0, and it copies
  such a cell before its first write. An all-constant list or dict literal can be one of those
  static cells, costing nothing until it is written, as adr:0003-value-semantics-with-reference-counting
  and adr:0008-value-semantics-with-reuse-before-borrowing allow. Starlark spends dedicated
  opcodes on a weaker version of this (`ListOfConsts`, `DictConstKeys`).
- **`s[i]` walks the string.** `lt_offset` counts code points from the start, so an index loop
  over non-ASCII text is quadratic. RustPython keeps a sparse code-point index (`Wtf8Index`) that
  makes indexing O(1).
- **Float printing loops.** `lt_shortest` tries up to 17 precisions through `snprintf` and
  `strtod`. Ryu finds the shortest representation directly. SPy vendors it with a normalizer to
  CPython's `repr` (`libspy/src/str.c`).
- **Sorting** is a merge sort. Timsort is stable, as the merge sort is, and faster on partially
  ordered data. RustPython's version follows CPython's `listsort`, which is the one to port
  (PSF-2.0).
- **Small dicts.** Starlark builds no hash index for a dict of up to 16 entries and scans
  instead. This applies to dicts only: sets must keep CPython's slot order.

Each of these is held to the benchmarks before and after, and to [[target-parity]].

## The Python target gets no optimization

LLVM optimizes `build` at `-O2`. `lotml run`, `lotml test` and every reinforcement-learning
rollout run on the Python target, and `lotml-ir` has no general constant folding (it folds only
set literals, to fix their slot order) and no inlining.

- **Starlark's recipe is the safe one.** It folds inside smart constructors using the runtime's
  own evaluator, and folds only results that cannot fail. It inlines `return <expr>` bodies under
  a size cap, keeping the callee's source position, and it marks which builtins are pure.
- **The prior art shows what goes wrong without that care.** SPy and LPython both moved errors
  from run time to compile time by folding: SPy turned a division by zero into a compile error in
  two of its three modes. LPython's `round(-2.7)` folds to −3 at compile time and returns −1 at
  run time.
- **For lotml,** folding must leave overflow and division by zero as run-time errors, and the
  parity suite runs with folding on and with it off.

Two costs sit outside the IR:

- **Loading.** `lotml_rt.py` rebuilds every module from JSON, through `ast` and `compile()`, on
  each load. erg caches the code object.
- **Latency.** Every `lotml run` starts a fresh CPython. monty's sub-millisecond latency comes
  from a pool of pre-started worker processes, not from its interpreter. A warm CPython pool
  behind `run`, `test`, the MCP `test` tool and the grader fits adr:0025.

## Passes worth their names

LPython runs its passes through a pass manager that can dump the IR after each pass and time each
one, and it prunes unreachable functions before code generation. SPy resolves every operator
through one table, `(op, left type, right type)` mapped to an implementation name, and its
backends implement those names by convention. lotml writes the same rule three times: in the
checker's `match` over operators (`lotml-check/src/body.rs`), in the IR's `Binary`, and in each
backend. One table read by all three is adr:0020-one-ir-between-the-checker-and-every-backend's
idea taken one step further.

The work is in `plans/build-and-check-speed.md`, `plans/runtime-hot-paths.md` and
`plans/ir-passes.md`.
