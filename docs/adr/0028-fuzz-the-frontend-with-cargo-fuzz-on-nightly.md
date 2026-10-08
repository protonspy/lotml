---
status: accepted
---

# 0028 · Fuzz the frontend with cargo-fuzz, on nightly, outside the workspace

## Context

The language and MCP servers feed the parser, the checker and the lowering whatever an editor or
an agent has half written, and nothing fuzzed them (plans/frontend-robustness.md). A panic there
takes down the server an agent is working through; an endless loop hangs it. The hand-written
tests and the corpus cover the programs somebody wrote, and the token soup of
`lotml-syntax/tests/termination.rs` reaches only the parser.

Three ways to fuzz were weighed:

- **cargo-fuzz over libFuzzer**: coverage-guided, the tool ruff fuzzes its parser with
  (`research/prior-art/studies/ruff.md`). It needs a nightly toolchain, and on this Windows
  machine it does not link: MSVC's AddressSanitizer runtime is missing, and without a sanitizer the
  coverage sections libFuzzer reads (`__start___sancov_pcs`) are unresolved. It builds and runs on
  Linux, including WSL.
- **AFL through afl.rs**: also coverage-guided, Unix only, and a second engine to learn for no gain
  over libFuzzer.
- **A mutation fuzzer written here**: runs on the stable toolchain everywhere, but blind to
  coverage, so it finds less in the same time, and it is code to keep.

## Decision

The frontend is fuzzed with **cargo-fuzz** over **libFuzzer** (`libfuzzer-sys`), in
`compiler/fuzz/`, a crate kept out of the workspace with its own `[workspace]` table. The compiler
itself stays on the pinned stable toolchain (adr:0006-compiler-written-in-rust); only the fuzzer
builds on nightly. One target, `frontend`, takes any UTF-8 text through `check_source`,
`check_prefix`, the Python target's compile and the LLVM target's compile with and without tests
and line tables, on a worker thread with the 256 MiB stack the `lotml` binary gives the compiler.
Its first inputs are the corpus's programs, which `cargo run --bin seed` writes out. On Windows it
runs under WSL; `compiler/fuzz/README.md` has the commands.

## Consequences

- A run is coverage-guided and reproducible from the artifact libFuzzer writes for a crash, which
  becomes a test where the bug is fixed.
- Running it needs nightly and Linux or macOS; a Windows contributor uses WSL. CI does not run it:
  a fuzzing run is a search, not a gate, and a time-boxed one in CI would fail at random on what it
  happened to find.
- `libfuzzer-sys` and `serde_json` are dependencies of the fuzz crate only; nothing the compiler
  ships links them.
