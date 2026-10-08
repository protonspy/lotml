---
autonomy: auto
ci: wait
pr: per-group
merge: manual
status: approved
checksum: b480f2420071f86488c1b9bc556937246f449c5208638fe37de2ed271f1850d4
---

# Build and check speed

Stop recompiling the runtime on every native build, name a missing toolchain once and clearly,
and check per function rather than per file so an edit costs what it touched.

## Why

The runtime's `lotml.c` is compiled again on every `lotml build` and every LLVM test, which is
60–80% of a small build (`see n-0091`).

`lotml-db` checks a whole file per query and re-parses every interface on each check
(`see n-0096`), against the under-100 ms check-on-edit requirement. ty runs the same salsa 0.28 at
per-definition grain, and mun separates a function's signature from its body.

Done when a second native build reuses the runtime object, a body edit re-checks only that
function, and both are measured before and after. See `docs/wiki/pages/compiler-performance.md`.

## Paths

- `compiler/crates/lotml-llvm/src/driver.rs`
- `compiler/crates/lotml/src/exec.rs`
- `compiler/crates/lotml-db/src/lib.rs`
- `compiler/crates/lotml-check/src/`
- `docs/wiki/pages/transpilation-strategy.md`
- `specs/`

## References

- adr:0021-compiler-in-rust-with-llvm-as-its-native-code-generator
- adr:0025-two-targets-python-for-run-llvm-for-build — "the C runtime … compiled by `clang` with every program"; caching keeps the runtime a separate unit and changes only when it is compiled
- `docs/wiki/pages/compiler-performance.md`
- `research/prior-art/studies/mun.md`, `ruff.md`, `plix.md`

## Out of scope

- Cranelift, a linked LLVM or an embedded linker: ruled out by adr:0021 and adr:0025.
- Link-time optimization between program and runtime: `plans/runtime-hot-paths.md` inlines the hot calls instead.

## Tasks

- [x] 1.1 (Unit) Measure a small native build at `-O0` and `-O2`, split into runtime compile, program compile and link, and record it in `harness/results/`
- [x] 1.2 (Unit) Cache the runtime's object keyed by a hash of the runtime sources, `clang`'s path and version, the optimization, sanitizer and shared-library flags, and the target; rebuild on a miss only. The cache lives in a per-user directory with owner-only permissions, never a shared fixed path; an entry is written to a temporary file and renamed into place, and its hash is checked before it is linked. Record in `docs/wiki/pages/transpilation-strategy.md` that the runtime is compiled once per toolchain and flags, citing adr:0025
  _Depends 1.1_
- [x] 1.3 (Unit) Probe `clang` once per process and turn a missing linker or Windows SDK into a diagnostic that names what is missing, rather than passing clang's raw output through
- [x] 2.1 (Unit) Measure check latency after a one-line body edit in a large file through `lotml-db`, and record it as the baseline
- [ ] 2.2 (Unit) Make each file's interfaces a tracked salsa input with high durability, so a check no longer re-parses them
  _Depends 2.1_
- [ ] 2.3 (Unit) Specify, as a new spec named incremental-check, per-function check queries over signatures, spans relative to their item, and the diagnostics a file reports assembled from them
  _Depends 2.1_

## Done when

- A second `lotml build` of the same program with the same flags does not compile `lotml.c`, shown by a test.
- `harness/results/` holds the build and check measurements before and after.
- The incremental-check spec exists, and `scc validate` exits 0.
- `cargo test --manifest-path compiler/Cargo.toml` and clippy pass.
