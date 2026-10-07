---
status: accepted
---

# 0025 · Two targets: Python for `run`, LLVM for `build`

## Context

adr:0021-compiler-in-rust-with-llvm-as-its-native-code-generator made LLVM the native code
generator and retired the C target once the LLVM target passed the parity suite, keeping it until
then as a native oracle; adr:0023-native-programs-load-cpython-at-run-time-through-its-stable-abi
had native programs load CPython so they could import Python modules. Building the first
increment of the LLVM backend showed what the oracle costs: making the runtime take the place of
a panic by pointer, which LLVM IR needs (specs/llvm-backend R2.7), meant changing the C emitter
too, and every later change to the runtime would mean the same — a second native backend kept
compatible for the length of the parity work, for a check the Python target already gives, since
parity is defined against the Python target. The bridge to CPython was a third mechanism for what
the Python target already does: a program that needs numpy, FastAPI or any other Python library
runs on CPython under `lotml run`, with the whole ecosystem and no embedding, isolation or
library discovery to get right. The user asked for the simpler shape: `run` on CPython, `build`
to native code, nothing else.

## Decision

LotML has two targets. `lotml run` and `lotml test` compile to Python for CPython, where a program
reaches Python libraries through interfaces as
adr:0012-python-interop-through-checked-boundaries-and-interface-files decides; `lotml build`
compiles to native code through LLVM, as adr:0021 decides it — Rust, the hand-written parser,
Salsa, textual LLVM IR compiled by `clang` with the C runtime, `-O0` and `-O2`, no Cranelift, no
linked LLVM — and `--target llvm` is the only native target. The C target is retired now, after
the LLVM backend's first increment, not after parity: the parity suite runs on the Python and LLVM
targets, the Python target its reference. A native program does not load CPython: `build` refuses
a Python import at the import, with a diagnostic pointing at `lotml run`. A native build may still
be a library C calls (adr:0024-c-abi-exports-chosen-by-signature-without-new-syntax). Rejected:
keeping the C target until LLVM parity (a second emitter every runtime change must reach, for an
oracle the Python target already is); and embedding CPython in native programs (adr:0023's bridge,
for what `lotml run` already gives).

## Consequences

- Until the LLVM target compiles what the C target did (specs/llvm-parity), a program using
  strings beyond literals, collections, records or closures has no native build; it runs under
  `lotml run`. That gap is the parity work, measured by the parity suite as it closes.
- The C runtime stays: it is the library the LLVM backend's code calls, compiled by `clang` with
  every program, not a target.
- A program that imports a Python module is a `run` program; to build it natively its Python part
  is replaced by LotML or by a C library (adr:0013-c-libraries-through-interfaces-named-c).
- The benchmarks measure the LLVM target against hand-written C; the C target's recorded runs
  stay as history.
