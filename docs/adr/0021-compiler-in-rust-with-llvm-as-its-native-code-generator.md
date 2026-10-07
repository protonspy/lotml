---
status: accepted
---

# 0021 · Compiler in Rust, with LLVM as its native code generator

## Context

adr:0006-compiler-written-in-rust chose Rust, a hand-written parser and Salsa, and planned two
native code generators once the native backend arrived: Cranelift for debug builds, LLVM for
release. Its own consequences weighed against the first: Cranelift's gain in rustc was about 5%
of a clean build, and the speed of the agent's loop depends on the incremental frontend more than
on the backend. Since then the C target (adr:0016-c-target-as-monomorphic-c-over-a-counting-runtime)
has given native executables through whichever C compiler is installed, and every backend is
moving onto one IR (adr:0020-one-ir-between-the-checker-and-every-backend). The native backend
arrives now (`plans/ir-architecture.md`), and each code generator over the IR is one more
emitter that every construct of the language has to reach.

LLVM is reachable two ways from Rust. Linking its libraries (`llvm-sys`, `inkwell`) puts one
pinned LLVM version's development files on every machine that builds the compiler and links LLVM
into the `lotml` binary. Writing LLVM's textual IR and handing it to `clang` needs only an
installed toolchain, found the way CPython and the C compiler already are; `clang` also compiles
the C runtime the native code calls into. The development machine had no LLVM toolchain when this
was decided — clang 23.1.3 was installed for this work afterwards, outside `PATH`; CI's Ubuntu
runners ship `clang`.

## Decision

The compiler stays in Rust, with the hand-written parser, Salsa and a separate tree-sitter grammar
for editors, as adr:0006 decided and for its reasons, Zig and a compiler written in LotML still
rejected. LLVM is its native code generator: the LLVM backend writes textual LLVM IR from the
counted IR and `clang` compiles it, with the C runtime, into an executable — `-O0` for a debug
build, `-O2` for release. Cranelift is dropped. Rejected: Cranelift for debug builds (a second
emitter for a gain the frontend, not the backend, decides); linking LLVM through `inkwell` or
`llvm-sys` (every compiler build tied to one LLVM release's libraries, and checked IR construction
is not worth that here, since the IR is written from a typed program and malformed output is a
backend bug the tests find); the C target as the only native target, which leaves overflow checks,
layout and optimisation to whichever C compiler is found; and keeping the C target beside LLVM, as
the default's alternative or behind a flag of its own, which is a second emitter every construct of
the language has to reach and a second native result to hold in parity, for a fallback
adr:0022-lotml-build-makes-a-native-executable-by-default already rejects. The C target is retired
once the LLVM target passes the parity suite; its runtime stays, compiled by `clang` with the IR
the LLVM backend writes.

## Consequences

- A native build through LLVM needs `clang` on the machine; it is found, never linked, and a
  missing one is a diagnostic naming the targets that do not need it. Whatever `clang` resolves
  to runs with the user's rights, so it is looked up from an explicit variable and then `PATH`,
  never the working directory, and given its arguments as a vector, never through a shell.
- Overflow and index checks are safety properties, not only parity: the backend writes them
  with LLVM's checked-arithmetic intrinsics rather than relying on behaviour `-O2` may assume
  away.
- Nothing checks the textual IR before `clang` reads it, so an emitter bug surfaces as a `clang`
  error; the backend's tests compile and run what it writes.
- Textual IR changes between LLVM releases; the backend writes the opaque-pointer form and names
  the oldest `clang` it supports.
- Until it is retired, the C target shares the counted IR and the C runtime with LLVM and the
  parity suite runs on both, so the LLVM emitter is checked against a native target that already
  passes. Retiring it removes `--target c`, the C emitter and its C compiler discovery; the
  runtime stays C, compiled by `clang`.
- The roadmap's task 5.1 and the wiki's account of Cranelift for debug builds describe a plan
  this record replaces.
