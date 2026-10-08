# Rust binder — tasks

## 1 · The stubs lotml carries

- [x] 1.1 (Unit) Vendor typeshed's `stdlib`, `LICENSE` and the pinned commit under `compiler/crates/lotml-bind/typeshed/`, with `release/typeshed.py` to replace them, and record typeshed and `miniz_oxide` in `docs/stack.md` — R2.1
- [x] 1.2 (Unit) Embed the vendored stubs deflated in the `lotml-bind` crate, and look a module up by name, gated by `VERSIONS` for CPython 3.14 — R2.1, R2.3
  _Depends 1.1_

## 2 · The binder

- [x] 2.1 (TDD) Port the Python binder to `lotml-bind`, reading a stub with `ruff_python_parser`: the type map, the defaults with Python's `repr`, version blocks, aliases, overloads and coroutines, and one escape function for every string taken from the stub — R1.1, R1.3
- [x] 2.2 (Unit) Refuse a stub past 8 MiB, past the nesting or tokens-per-line limit, or that does not parse, before walking it, on a thread with a stack those limits fit — R1.4
  _Depends 2.1_
- [x] 2.3 (TDD) Show the Rust binder writes the Python binder's interface for every module of the vendored `stdlib` and every PyPI stub of the coverage corpus, and keep goldens of the corpus's standard-library interfaces — R1.2
  _Depends 1.2, 2.2_

## 3 · `lotml bind` without Python

- [x] 3.1 (Unit) Have `lotml bind` run the Rust binder in-process: a given stub, else typeshed's, else the project's packages for a name that is not the standard library's; drop its `--offline` flag, `lotml_bind.py` and `lotml_py::BIND` — R1.1, R2.2, R2.3, R2.4
  _Depends 2.3_
- [x] 3.2 (Unit) Have the binding coverage report bind through `lotml bind`, and record a measurement under a new label beside the last — R3.1
  _Depends 3.1_
- [ ] 3.3 (Unit) Update the wiki page on transpilation and the glossary for a binder that runs no Python — R1.1, R2.2
  _Depends 3.1_
