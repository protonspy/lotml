# Transpilation strategy

The original study recommends transpiling to Python first (validate the design in weeks), then to
C (reach the performance target) and only then writing a native backend; and, separately,
transpiling Python to lotml to generate a corpus. The order holds. The evidence adds the price of
each semantic gap between lotml and Python, and a consequence the study did not draw: the
transpiler to Python is what lets the harness execute programs.

## Python as the first target

**Integers.** Python has arbitrary-precision integers; lotml has `i64` with defined overflow.
Every Python compiler diverges here: Cython keeps `int` as a Python object because C "can be
quite different with respect to overflow and division", Codon uses 64 bits, and mypyc leaves
`i64` overflow undefined. Emulating a trap costs one check per operation — in a rough local
measurement taken during this research (CPython 3.13, Windows), about 2.2× on an addition, and
3.4× to emulate wrapping. In phase 1 the Python target exists to validate the design, not for
performance, so an emulated trap is acceptable. There must be a single semantics — see
[[type-system]].

**Value semantics.** Python's lists and dictionaries are references, and there is no built-in
copy-on-write list. Since everything is immutable by default, only a value entering a `var` or an
`inout`/`sink` parameter needs a copy ([[memory-model]]); immutables can be shared because the
compiler guarantees nobody changes them. Costlier alternatives: persistent structures
(pyrsistent, log32 access) or 3.15's `frozendict` (PEP 814).

**Concurrency.** Python has no faithful green threads; since concurrency is v2, tasks become
threads — see [[colorless-concurrency]].

**Errors pointing at the source.** Nodes of Python's `ast` module carry `lineno` and
`col_offset`, and `compile()` accepts an AST. Emitting an AST with the original positions should
make tracebacks point at the `.lotml` file — an inference from the documentation, to be verified
in the harness. Hy compiles to Python AST, and Coconut keeps line numbers with `--line-numbers`.
Haxe and Coconut also target Python.

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
keep only what passes the tests ([[training-prior]]). The paired corpus shows that, from typed
Python to lotml, much of the mapping is mechanical (a dataclass becomes a record, exception
classes become a sum type, `Optional` becomes `T?`). What is not mechanical is deciding which
functions fail: it requires following `raise` statements transitively. The workable design is
hybrid — rules for the mechanical part, an LLM for the rest, tests to validate.

## Recommended order

The original study's order holds, with one thing moved earlier: a minimal transpiler to Python
enters in phase 0, because without executing programs the [[evaluation-harness]] only measures
parsing and static rules, like the [[python-leakage-pilot]]. The same transpiler, grown, is
phase 1.
