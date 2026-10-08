# Prior-art compilers

Sixteen open-source projects run or compile Python, or a language derived from it, and most of
them have a compiler written in Rust. In October 2026 they were read against lotml's compiler to
look for architecture, optimizations, parts to reuse and mistakes already paid for. One study per
project, each citing the source line by line, is in `research/prior-art/studies/`. The
cross-cutting result is in `research/prior-art/synthesis.md`, and the clones' commits are in
`research/prior-art/README.md`.

## The projects

| project | what it is | closest to lotml in |
|---|---|---|
| pon | Python 3.14 through one IR to Cranelift: a JIT for `pon run`, AoT objects linked into an executable for `pon build` | one IR feeding two execution modes; a parity harness with ratcheted floors |
| pycc | Typed Python 3.14 through ruff's parser, a type checker and a tree MIR to LLVM through inkwell | typed, LLVM, Rust, written by agents |
| SPy | Static Python with an interpreter, a redshift pass and a C/WASM backend, written in Python | the same tests run in every mode; operators resolved through one table |
| plix | A gradually typed language with an interpreter and a Cranelift compiler | two backends that share only a parser |
| interpreter-rs | An interpreter and a compiler that share a parser | the same, abandoned halfway |
| monty | pydantic's sandboxed Python interpreter on ruff's parser, for code written by LLMs | LLM-written code, a worker pool, resource limits |
| edge-python | Single-pass SSA and a tiered register VM for a sandboxed Python subset | resource metering |
| RustPython | Python interpreter with a bytecode compiler and an experimental Cranelift JIT | runtime algorithms: string index, Timsort, format specs |
| ruff / ty | Python linter and type checker; parser, AST, and salsa-based checking | tolerant parsing; salsa 0.28 at per-definition granularity |
| erg | Statically typed Python-compatible language, compiled to Python bytecode or transpiled to Python | the Python target; declarations for Python modules |
| mamba | Strictly typed Python-like language transpiled to Python | the Python target |
| depyler | Annotated Python transpiled to Rust | ownership inferred from Python code |
| tarvos | A Python subset transpiled to Rust, then built by rustc | `run`, `build` and `compile` commands |
| mun | Statically typed embeddable language: salsa, HIR, LLVM through inkwell, hot reload | salsa layering; shared libraries for a host |
| starlark-rust | Meta's interpreter for Starlark, a deterministic Python dialect | safe constant folding and inlining; constant literals |
| LPython | Typed Python through ASR, an IR shared with LFortran, to LLVM, C and WASM, in C++ | one IR for several backends |

## Verdict

**No project has a compiler core worth importing.** On the points where they overlap, lotml's
choices hold up better:

- **Arithmetic.** pon's optimizing tier and RustPython's JIT lower Python's `//` and `%` to C's
  truncating division. RustPython's tests even assert the wrong result. lotml's emitters get both
  right.
- **Toolchain.** pycc paid for inkwell and a linked LLVM with a pinned LLVM install, vcpkg
  `libxml2` on Windows, a verifier skipped on Windows and a binary that cannot be relocated.
  pon and plix have no working Windows path at all.
  adr:0021-compiler-in-rust-with-llvm-as-its-native-code-generator and
  adr:0025-two-targets-python-for-run-llvm-for-build avoid all of it with textual IR and a
  `clang` found or provisioned at build time (adr:0027).
- **One IR.** Projects whose backends share only a parser drift apart:
  - plix disagrees with itself on overflow, scoping and `Result`;
  - interpreter-rs's compiler stalled behind its evaluator;
  - LPython shares its IR with another language, and after one sync 188 of its 400 integration
    tests were commented out.

  All three confirm adr:0020-one-ir-between-the-checker-and-every-backend.
- **Memory.** pon's tracing collector costs a write barrier per store, a shadow stack slot per
  local and imprecise roots in AoT code. That is the bill adr:0003-value-semantics-with-reference-counting
  and adr:0008-value-semantics-with-reuse-before-borrowing avoid. pycc never frees containers.

What the projects do offer lies around the core:

- how to prove that two targets agree — [[target-parity]];
- how to make builds, checks and hot runtime paths fast — [[compiler-performance]];
- a few runtime algorithms with clean licenses;
- a list of pitfalls.

## Not adopted, and why

| idea | seen in | ruled out by |
|---|---|---|
| Cranelift as a fast debug backend, or a JIT for `run` | pon, plix, RustPython | adr:0021, adr:0025 — and the runtime, not code generation, dominates build time ([[compiler-performance]]) |
| Linking LLVM or LLD into the compiler | pycc, mun | adr:0021 |
| A tracing collector | pon, mun | adr:0003, adr:0008 |
| An interpreter for `run`, in Rust or Starlark style | monty, starlark-rust, RustPython | a third copy of every builtin beside the Python runtime and the native one (adr:0025 keeps two targets). Even monty runs code out of process, because a stack overflow kills its host. |
| Emitting CPython bytecode | erg | one opcode table per CPython version, outside adr:0025's two targets, and erg is stuck at 3.7–3.11 |
| Compile-time "blue" evaluation, and generics checked only when instantiated | SPy | adr:0004-python-syntax-where-semantics-match and lotml's generics checked at definition: type errors would surface late |
| Records passed to C as counted handles | mun | adr:0024-c-abi-exports-chosen-by-signature-without-new-syntax, which rejected it |
| Replacing `lotml-syntax` with ruff's parser | pycc, monty, RustPython | the languages differ at the token level (`fn`, `var`, `fail`, `T ! E`, `??`). Ruff's AST is generated and declared unstable. A fork would mean about 50,000 lines to keep in place of 3,400, and even RustPython forks it and adds 2,600 lines. The recovery techniques are borrowed instead ([[semantic-compiler]]). |

## Code that may be copied

| item | from | license | for |
|---|---|---|---|
| Ryu shortest float formatting, plus a normalizer to CPython's `repr` (`spy/libspy/src/str.c`) | SPy | MIT; Ryu is Apache-2.0 or BSL-1.0 | the runtime's float printing |
| Code-point index over UTF-8 (`common/src/wtf8_index.rs`) | RustPython | MIT | O(1) `s[i]`, ported to C |
| Timsort; RustPython's `crates/vm/src/sorting.rs` follows CPython's `listsort` | CPython (`Objects/listobject.c`) | PSF-2.0 | list sort, ported to C from CPython |
| Format-spec parser (`common/src/format.rs`) | RustPython | MIT | checking f-string specs at compile time |
| PE and ELF readers (`src/embed/pe.rs`, `elf.rs`) | pycc | MIT | tests on built executables |

Copying keeps the original license text and a note of the changes. pon and interpreter-rs carry
no license, and tarvos is GPL-3.0 although its `pyproject.toml` says MIT: those are read for
ideas, never copied.

## Lessons for a project written by agents

- **pycc built process faster than it built the compiler.** In about 75 days it accumulated 255
  decision records, 7.1 MB of documents and some 60,000 lines of governance scripts. It then rolled
  much of that back because the process was eating each run's budget. Its architecture document
  describes crates that do not exist.
- **pycc's typed tree has no source positions.** It builds an empty span at 207 sites, so type
  errors point at line 1, column 1. For a language whose reader is a model repairing its own code,
  that is the failure to avoid most. lotml keeps a span on every node and every IR statement.
- **DeepWiki is a map, not evidence.** It describes plix's JIT, whose output is never executed, as
  a working tiered JIT.
- **Benchmarks must not be visible to the compiler.** plix once special-cased functions named
  `fib`.
