---
status: accepted
---

# 0022 · `lotml build` makes a native executable by default

## Context

Every command defaults to the Python target today: `lotml build` writes a Python module with the
runtime it imports, and an executable takes `--target c`. That was right while Python was the
only target (adr:0001-transpile-to-python-first). The project's direction now gives the two
worlds different jobs: Python for development, debugging and the Python ecosystem; native code
for performance and distribution, with a small runtime and no interpreter to ship. `run` and
`test` are the development loop, where a model iterates and the Python ecosystem is reachable;
`build` is what is distributed. The harness drives `check`, `run` and `test`, not `build`; the
README, the wiki and adr:0012-python-interop-through-checked-boundaries-and-interface-files
describe `build` as writing Python.

## Decision

`lotml build` compiles to an executable through the LLVM target
(adr:0021-compiler-in-rust-with-llvm-as-its-native-code-generator); `lotml run` and `lotml test`
keep the Python target; `--target python`, `--target c` and `--target llvm` stay explicit on all
three. Where `clang` is missing, `build` stops with a diagnostic that names `--target c` and
`--target python`. The default changes only once the LLVM target passes the parity suite.
Rejected: falling back to the C target when `clang` is missing, which makes what a command
produces depend on the machine it runs on, the kind of hidden variation the language exists to
remove; and keeping Python as the default, which leaves native code behind a flag in the one
command meant for distribution.

## Consequences

- Anything that relied on `lotml build` writing a Python module passes `--target python`; the
  documents that describe `build` that way are corrected in the same change.
- `build` with no flag fails on a machine without `clang` where it used to succeed.
- A program importing a Python module cannot be built natively until native targets can call
  CPython; until then `build` refuses it at the import, as the C target does.
- `run` and `build` default to different targets, so a program can pass under `run` and fail to
  build; the diagnostic names the target, and `run --target llvm` reproduces it.
