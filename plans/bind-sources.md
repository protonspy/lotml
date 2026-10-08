---
autonomy: auto
ci: wait
pr: per-group
merge: manual
status: approved
checksum: c785ac791afe3364ac8dd9a3558819e6f14c86db71598967d6e80357ca0eb069
---

# Bind sources

Let `lotml bind` type a Python module from more places than typeshed. It should read a package's
own stubs and annotations, and use the project's virtual environment. Whether to read stubs
without any Python at all goes to a proposed ADR.

## Why

`lotml bind` reads typeshed's stub from an installed mypy or jedi, or one passed with `--stub`.
A package that ships its own `.pyi` or annotated `.py` cannot be bound without hand work, and
`lotml run` uses whichever Python it finds first, not the project's environment.

- **erg** falls back to a package's own declarations.
- **monty** reads Python with `ruff_python_parser` and embeds a trimmed typeshed, needing no
  interpreter. adr:0012-python-interop-through-checked-boundaries-and-interface-files says bind
  reads stubs "with Python's own parser", so that change is the owner's call.

Done when bind reads package stubs and annotations, the venv is used, and the ADR is proposed.

## Paths

- `compiler/crates/lotml/src/exec.rs`
- `compiler/crates/lotml/src/main.rs`
- `docs/adr/`

## References

- adr:0012-python-interop-through-checked-boundaries-and-interface-files
- adr:0025-two-targets-python-for-run-llvm-for-build
- `research/prior-art/studies/erg.md`, `monty.md`

## Out of scope

- Trusting declarations unchecked, as erg's `.d.er` files are: every value still crosses the boundary checked, per adr:0012.
- Native programs importing Python: adr:0025 refuses it.

## Tasks

- [x] 1.1 (Unit) When typeshed has no stub for a module, read the package's own `.pyi`, then its annotated `.py`, statically and never by importing it, and say in the interface which source it came from
- [ ] 1.2 (Unit) Prefer the interpreter of the project's virtual environment for `lotml
      run` and `lotml test`, after `LOTML_PYTHON`: `VIRTUAL_ENV`, then a `.venv/` that
      resolves inside the project root without a link out of it; `lotml bind`, the MCP
      tools and the grader keep the interpreter they find today
  _Status removed_
  _Reason folded into plans/python-via-uv.md 1.3, which resolves the venv as one step of the interpreter order adr:0026 decides_
- [ ] 2.1 (Unit) Write a proposed ADR on reading stubs without Python
  _Status removed_
  _Reason adr:0032 is this record: it decides reading stubs without Python, ruff_python_parser and an embedded typeshed, and was accepted_

## Done when

- `cargo test --manifest-path compiler/Cargo.toml` passes, with tests binding a package stub and an annotated module.
- The ADR exists with `status: proposed`, and `scc validate` exits 0.
