# Transpilation strategy

The original study recommends transpiling to Python first (validate the design in weeks), then to
C (reach the performance target) and only then writing a native backend; and, separately,
transpiling Python to lotml to generate a corpus. The order held until the native step: the C
target of phase 3 was retired in phase 4 for LLVM, so two targets remain, Python for `run` and
`test` and native code for `build`, both reading one IR
(adr:0025-two-targets-python-for-run-llvm-for-build). The evidence adds the price of
each semantic gap between lotml and Python, and a consequence the study did not draw: the
transpiler to Python is what lets the harness execute programs. A research transpiler for the
pilot's subset, in `research/experiments/transpiler/`, now does that, and settles three of the
questions below by measurement.

## Python as the first target

**Integers.** Python has arbitrary-precision integers; lotml has `i64` with defined overflow.
Every Python compiler diverges here: Cython keeps `int` as a Python object because C "can be
quite different with respect to overflow and division", Codon uses 64 bits, and mypyc leaves
`i64` overflow undefined. Emulating a trap costs one check per operation. Measured in
`research/experiments/overflow/` (CPython 3.13, Windows, best of 7, no overflow taken):

| strategy the transpiler emits | `a + b` | `a * b` | loop `t = t + x` |
| --- | ---: | ---: | ---: |
| inline check after the operation | 2.40× | 2.20× | 2.43× |
| walrus in one expression | 2.84× | 2.63× | 2.82× |
| call to a `trap` helper | 3.61× | 3.23× | 3.62× |
| inline two's-complement wrap | 3.37× | 3.02× | 3.31× |

Inline checks are the cheapest — about 2.4× — and a helper call costs half as much again, so the
transpiler should emit the check inline. In phase 1 the Python target exists to validate the
design, not for performance, so an emulated trap is acceptable. There must be a single semantics —
see [[type-system]].

**Value semantics.** Python's lists and dictionaries are references, and there is no built-in
copy-on-write list. Since everything is immutable by default, only a value entering a `var` or an
`inout`/`sink` parameter needs a copy ([[memory-model]]); immutables can be shared because the
compiler guarantees nobody changes them. Costlier alternatives: persistent structures
(pyrsistent, log32 access) or 3.15's `frozendict` (PEP 814). The research transpiler copies a
mutable value wherever it is read from a place and bound or passed — the simple rule — and the
pilot's programs pass their tests under it ([[python-leakage-pilot]]); the same programs run with
Python's reference semantics show where the difference is observable. Two details the transpiler
had to settle: dictionary views (`items()`, `keys()`) become lists, a snapshot, because a view
cannot be copied; and fieldless variants are singletons that are never copied.

**Concurrency.** Python has no faithful green threads; since concurrency is v2, tasks become
threads — see [[colorless-concurrency]].

**Errors pointing at the source — verified.** Nodes of Python's `ast` module carry `lineno`,
`col_offset` and their end positions, and `compile()` accepts an AST and a file name. In
`research/experiments/tracebacks/`, a module compiled from an AST carrying lotml positions
produces a traceback that names the `.lotml` file and line, shows the lotml source line, and
underlines exactly the lotml expression that failed — even `10 // (d ?? 0)`, which Python cannot
parse; compiled from the generated Python's own positions, the same failure points at a line
that does not exist in the `.lotml` file. Two conditions: positions are UTF-8 byte offsets, so the
transpiler converts the parser's character columns; and when the `.lotml` file is not on disk,
registering its text in `linecache` is enough. Every node of the research transpiler carries its
lotml position, so the one failing pilot test reports `a-haiku/task-07.x, line 26` with the original line. Hy
compiles to Python AST, and Coconut keeps line numbers with `--line-numbers`. Haxe and Coconut also
target Python.

## The Python ecosystem seen from lotml

- **No language generates typed bindings from `.pyi` stubs.** Erg ignores Python's type hints and
  requires hand-written declarations, Codon asks for manual signatures, Mojo treats Python objects
  as dynamic. Generating bindings from typeshed (which mypy, pyright, PyCharm, Pyrefly and ty
  consume) is an opportunity, with a drift risk: typeshed warns that "any version bump can
  introduce changes".
- **Stubs do not declare exceptions.** So every call into Python can fail, and the generated
  binding returns `T ! PyError`. It is the honest price of having errors in the type.

## C as the second target

- **Precedents:** Nim (C, C++, Objective-C and JS), Koka (C with mimalloc and Perceus, no
  collector), Lean 4 (C), Vala (C/GObject, counting with `weak`), Chicken Scheme (CPS on the C
  stack).
- **Error mapping:** `#line` directives, made so that generators like bison send errors and the
  debugger back to the original source; Nim emits them with `--lineDir`.
- **Signed overflow is undefined behavior in C**, and the compiler folds `(a+1)>a` to true. The
  generator must emit checked-arithmetic builtins (or `-ftrapv`).
- **Rust as a target is still not recommended**, as in the original study.

## Compile time

- **The backend weighs less than it seems.** In rustc, Cranelift cut about 20% of code generation
  time, which is about 5% of a clean build; LWN measured −20% wall time on a debug build.
- **The incremental frontend weighs more.** Roc rewrote its compiler from Rust to Zig (487 days to
  parity) and rebuilds 450,000 lines in about 35 ms, against 3.4 s before. Salsa (0.28.5, still
  labeled "experimental") backs rust-analyzer with the invariant "typing inside a function's body
  never invalidates global derived data" — the property the under-100 ms check target requires.

## Python to lotml: the corpus

The measured path is [MultiPL-T](https://arxiv.org/abs/2308.09895)'s: translate with an LLM and
keep only what passes the tests ([[training-prior]]). Its limits apply here: it was measured only
on languages already in the model's pretraining data, with the low-resource model itself
translating, and its authors expect it not to work as is for a language the model never saw —
lotml's case; rejection sampling yields a working program for only about 30% of prompts even with
50–100 attempts ([Agnostics](https://arxiv.org/abs/2508.04865)), and each target needs a
hand-written translator for tests and signatures. For lotml the translator is therefore a rule-based
transpiler plus a prompted frontier model, validated by tests. The paired corpus shows that, from typed
Python to lotml, much of the mapping is mechanical (a dataclass becomes a record, exception
classes become a sum type, `Optional` becomes `T?`). What is not mechanical is deciding which
functions fail: it requires following `raise` statements transitively. The workable design is
hybrid — rules for the mechanical part, an LLM for the rest, tests to validate.

The pipeline exists (`harness/lotml_harness/corpus/`): the rules write an expression as Python
writes it, where variant B's syntax is Python's, and rewrite what differs by rule — `def` to
`fn` with the task's types, `var` for a local assigned again or changed in place and for a
parameter the body changes, the `typing` imports dropped, a collection's or a number's truth
compared explicitly. What they leave — `re`, a tuple assigned to names that change, a nested
function, an untyped helper — goes to a frontier model with the reason, then the compiler's
diagnostics or the failing tests. A program is kept only when it passes the task's hidden
tests, and is stored with them as a `test` block ([[training-prior]] has the counts).

## The phase 1 backend

`compiler/crates/lotml-py` writes Python's syntax tree as JSON, every node at its lotml position,
and a stub module that hands it to the runtime, which compiles it under the `.lotml` file's name.
With the checker's types it copies only where a `var` or an `inout` is involved — on entering one,
or leaving one into a binding, a container, a capture or a call that may keep the value, shallowly
when the elements cannot change — and traps integer arithmetic inline, after the statement when the
result is assigned. On the 694 stored variant B answers the checker accepts, it gives the same
verdict on the hidden tests as the phase 0 transpiler. Since phase 4 it writes that tree from the
generic IR rather than from the syntax tree, the copies decided by the same rule on the IR's
locals, and reports what it reported before on the whole corpus.

## The phase 3 backend

`compiler/crates/lotml-c` compiled a checked module to one C11 file over a reference-counting
runtime (adr:0016-c-target-as-monomorphic-c-over-a-counting-runtime), until phase 4 retired it
for the LLVM target. Lowering gave a typed, monomorphic form, a generic function compiled once per
instantiation and a `dyn` value calling through a table; counts followed liveness, a dying value
was reused in place when it was unique, and `#line` directives made the C compiler, a debugger and
a panic name the `.lotml` line. `lotml test` reported on it what it reports on the Python target,
field for field: all 509 programs of the corpus reported the same on both targets
(`harness/results/parity.md`). Its lowering, its counting and its runtime outlived it, as the IR
and the runtime the native target runs on.

The benchmarks against C (`harness/results/benchmarks.md`) found the costs in the C the backend
writes rather than in counting: the location a failing check reports, built at each use, cost a
list store ten times its price under gcc until it became a static constant per line, and the
copy-on-write check moved out of loops that only store into a list. What remains is the checks
themselves, which the phase 3 gate measures ([[evaluation-harness]]).

## The phase 4 targets

One lowering in `compiler/crates/lotml-ir` turns the checked program into one IR, which both
targets read (adr:0020-one-ir-between-the-checker-and-every-backend): the Python target as
lowering leaves it, each generic function once, and the native target after `mono` has made it
monomorphic and the passes have inserted the counts, reuse and hoisted uniqueness checks of the C
target. `compiler/crates/lotml-llvm` writes it as textual LLVM IR, which `clang` compiles with the
same C runtime (adr:0021-compiler-in-rust-with-llvm-as-its-native-code-generator); the runtime's
signatures are read from its header, and line tables at `-O0` make a debugger name the `.lot`
line. `lotml build` makes an executable by default (adr:0022), and with `--shared` a library C
calls, exporting the functions whose signatures C can be given
(adr:0024-c-abi-exports-chosen-by-signature-without-new-syntax). A native program runs without
Python, so a Python module's import is refused there; a C library is called directly and linked
(adr:0013-c-libraries-through-interfaces-named-c).

The runtime stays a separate unit, as adr:0025-two-targets-python-for-run-llvm-for-build has it,
and is compiled once per toolchain and flags rather than with every program. Its object is kept in
a per-user cache readable by its owner alone (`LOTML_CACHE_DIR`, or the system's cache directory;
on Windows, where the profile's permissions are what keep it private, only inside the profile).
It is keyed by everything the compile reads: the runtime the compiler carries, compiled from a
private copy rather than the build directory, `clang`'s path and `--version`, the flags, and the
variables through which `clang` finds headers. An entry is written to a temporary file, renamed
into place, and checked against the SHA-256 its name carries before it is linked; the digest finds
a damaged entry, and the directory's ownership is what keeps out a planted one. The runtime was 58% of a small `-O0` build and 92% of an `-O2`
one; with its object cached, the build takes 35% and 9% of its time
(`harness/results/build-speed.md`).

The suite is still the corpus: all 509 programs report the same on the Python and LLVM targets
(`harness/results/parity-llvm.md`). Against hand-written C built by the same `clang`
(`harness/results/benchmarks-llvm.md`), mandelbrot takes 0.69x its time, fib 1.74x, sieve 2.26x,
collatz 2.97x and matmul 5.03x: the numeric gap the phase 3 gate recorded stays open, measured
rather than closed. [[compiler-performance]] lists the runtime paths behind it, and
[[target-parity]] how the suite that holds the two targets together can be made a gate.

## Python and lotml calling each other

Phase 2 built both directions of R14 and R27 on one checked boundary
(adr:0012-python-interop-through-checked-boundaries-and-interface-files):

- **Python calling lotml.** A compiled module loads its program into a namespace of its own and
  shows Python each function wrapped. The arguments are checked against the lotml signature —
  integer ranges, `int` widened to `f64`, element types, records and variants rebuilt field by
  field — and copied, so a caller's list is never shared. A function returning `T ! E` raises
  `lotml_rt.LotmlError` carrying the error. `lotml build --target python` writes a `.pyi` beside
  each module, with records as classes and a sum type as the union of its variants. `lotml run`
  and `lotml test` still see the program unwrapped.
- **A project's Python dependencies.** A project declares them in `pyproject.toml` and pins them in
  `uv.lock`, as Python's tooling writes both; lotml adds no format (specs/python-dependencies,
  adr:0033-lotml-run-installs-a-project-s-python-dependencies-from-its-uv-lock). `lotml run` and
  `lotml test` check the lock names PyPI alone — every package from its registry, every file from
  its host with a hash — and run the program in an environment made from the lock in lotml's cache,
  keyed by the lock, the manifest and the interpreter, by one `uv sync --frozen` of wheels only. A
  made environment is used offline; offline, and for the MCP server, the grader and the harness,
  nothing is installed. A manifest declaring dependencies with no lock is reported with `uv lock`.
- **lotml calling Python.** `lotml bind <module> --stub <file.pyi>` reads a stub with the Python
  parser lotml carries, Ruff's, and runs no Python (specs/rust-binder,
  adr:0032-a-python-module-is-bound-at-check-time-from-stubs-read-in-rust). With no stub given, a
  standard-library module's is typeshed's, embedded in lotml at a pinned commit and gated by
  typeshed's `VERSIONS` for CPython 3.14; any other module's is found in the environment made from
  the project's `uv.lock`, never `VIRTUAL_ENV` or `.venv`, in PEP 561's order — a
  `<package>-stubs`, a `.pyi` the package ships, or its own `.py` when it carries `py.typed` — read
  as a file and never imported. `import py.<module>` needs no `lotml bind`: `check`, `run`, `test`
  and both servers generate the interface on import, kept per user by the stub's hash, and
  `lotml bind --lock` records in `lotml.lock` what each module was bound from, which
  `check --locked` holds CI to (specs/bind-on-import). A stub is hostile input: one
  past 8 MiB, nesting deeper than 100 or with a line of more than 20 000 tokens is refused unparsed.
  It writes `bindings/py.<module>.lotmli`, whose first line names the source it was read from, an
  interface of bodyless signatures each returning `T ! PyError`;
  names typeshed writes as methods of a module-level instance, `randint = _inst.randint`, are
  bound from those methods.
  A class the stub defines is bound as a `class` block of the interface, its constructor, methods,
  static methods and attributes each `T ! PyError`, its value a handle to the Python object that
  only those members reach (specs/python-classes,
  adr:0034-a-python-class-crosses-as-a-nominal-handle-with-its-declared-members); a protocol and a
  class's dunder methods are listed in a comment. A type variable is a type parameter of what uses
  it, inferred at the call, its bound left to Python; a constrained one (`AnyStr`) is an overload
  per type; a generic class is a handle with type arguments, `Pattern[str]`, which Python erases,
  so what is checked is each value read through it (specs/python-generics,
  adr:0036-a-python-type-variable-crosses-as-a-type-parameter-checked-where-a-value-crosses). A union other than `X | None`,
  `Any`, a callable or a class the stub does not define is bound as `PyObject`, an opaque value a
  program passes back to Python or converts with `o.value()`, the boundary checking it
  (adr:0031-a-python-name-no-stub-types-crosses-as-an-opaque-python-value); an overloaded
  function, constructor or method is written once per overload, in the stub's order, and a call
  is typed by the first overload its arguments fit, a value given to a `PyObject` parameter
  fitting last (specs/python-overloads,
  adr:0035-a-python-overload-crosses-as-ordered-signatures-chosen-at-the-call); an optional parameter whose default is not a literal is
  written `= todo()`. A program imports the module by its origin, `import py.m` or `from py.m
  import f` (adr:0029-foreign-modules-are-imported-by-origin-and-their-interfaces-generated),
  which finds the nearest `bindings/py.m.lotmli` up the directory tree; a bare `import m` names a
  LotML module only. At
  run time any exception, and any returned value that does not match the declared type, is
  `Err(PyError(kind, message))`.
- **lotml calling C** (R17, adr:0013-c-libraries-through-interfaces-named-c). An interface named
  `c.<library>`, written by hand, declares C functions over what C passes by value — integers,
  `f32`, `f64`, `bool`, and `str` as a `const char*` argument — none of which can fail. On the
  Python target `ctypes` loads the library when the module loads, so a missing one stops the
  program as linking would, and releases the interpreter during a call, so a call that blocks
  holds up only its own task's thread. Calling C made the sized integers usable: a literal
  takes the integer type expected where it fits, and `i32(n)`, `u8(n)` and the rest convert.

## Recommended order

The original study's order holds, with one thing moved earlier: a minimal transpiler to Python
enters in phase 0, because without executing programs the [[evaluation-harness]] only measures
parsing and static rules. The research transpiler proved the point on the pilot: executing the 60
programs found a test that silently depends on reference semantics and two whose result depends on
variant A's semantics differing from Python's falsiness ([[python-leakage-pilot]]) — none of which parsing could see. The phase 1 transpiler is
a rewrite in the compiler's language (adr:0006-compiler-written-in-rust), not this research code.
