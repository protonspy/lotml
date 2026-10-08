---
autonomy: auto
ci: wait
pr: per-group
merge: manual
status: approved
checksum: 9ecf40683bb812640b18df5aedf45937a5697269e3b1b60f5c867dbd262d64be
---

# Python compatibility

Let a LotML program import any Python module as `py.<module>` under `lotml run`, without running
`lotml bind` first, and type more of each module's API with every spec. A name the stub cannot type crosses
as a dynamic Python value, used only through an explicit conversion, until a later spec types it.

## Why

Calling Python today takes a manual `lotml bind` per module, a stub found in an installed mypy
or jedi, and an API that fits the few shapes the binder types: module-level `def`s with simple
types. typeshed's `random` binds 0 functions, because its names are `randint = _inst.randint`
aliases, and classes, overloads, unions and generics — most of numpy, pandas or requests — are
left out. No prior project combines interfaces generated on import, values checked at the
boundary, and a dynamic value only for the remainder: Mojo and Codon keep every Python value
dynamic, Erg asks for hand-written declarations, and pyo3_bindgen trusts the annotations it
reads from the imported module. Importing first and typing progressively is the order that
reaches every module, and a coverage report over a fixed corpus measures the typing. Done when
every module of that corpus imports under `lotml run` with no `lotml bind`, and the report shows
the typed share each spec added.

## Paths

- `compiler/crates/lotml-py/runtime/lotml_bind.py` — the binder, until it is read in Rust
- `compiler/crates/lotml/src/exec.rs` — how `bind`, `check` and `run` reach a module's interface
- `compiler/crates/lotml-check/` — import resolution and the interface a check reads
- `compiler/crates/lotml-py/src/boundary.rs` — the checks a value crossing from Python passes
- `harness/lotml_harness/experiments/` — the binding coverage report
- `docs/adr/`

## References

- `specs/binding-coverage/` — a fixed corpus of modules and the share of their public API bound typed
- `specs/origin-imports/` — `import py.<module>`, and the bare import of a Python module refused
- `specs/python-object/` — the dynamic Python value, and the explicit conversion out of it
- `specs/bind-on-import/` — an import with no interface bound at check time, the result kept as a lock
- `specs/python-classes/` — Python classes as types, their methods and attributes typed
- `specs/python-overloads/` — overloaded functions and unions bound instead of skipped
- `specs/python-generics/` — generic functions and callables bound instead of skipped
- `plans/bind-sources.md` — package stubs, annotated modules and the project's environment as
  stub sources; its 2.1 ADR on reading stubs without Python is the one 2.3 extends
- `plans/python-via-uv.md` — the shipped uv and Python 3.14; its 2.1 takes typeshed from a locked
  mypy wheel, which 2.3 keeps or supersedes
- adr:0012-python-interop-through-checked-boundaries-and-interface-files
- adr:0025-two-targets-python-for-run-llvm-for-build
- adr:0026-lotml-ships-uv-and-runs-python-3-14-by-default
- `research/prior-art/studies/monty.md` — stubs read in Rust, typeshed embedded in the binary

## Out of scope

- Native programs importing Python: adr:0025-two-targets-python-for-run-llvm-for-build refuses it.
- Trusting a stub unchecked: every value still crosses the boundary checked, per adr:0012.
- Python calling LotML, which adr:0012 already settles.
- New LotML syntax: `class`, `try`, `with` and `yield` stay out; a Python class is used, not declared.

## Tasks

- [x] 1.1 (Unit) Bind the module-level aliases typeshed writes as `name = _inst.method`, from
  that method of the class the stub declares `_inst` with, and list one whose method the stub
  does not hold among the names not bound
- [x] 1.2 (Unit) Write and build the spec binding-coverage: a fixed corpus of the standard library
  and the most downloaded PyPI packages, the share of public names bound typed per module, and
  the report the harness writes
- [x] 1.3 (Unit) Write and build the spec origin-imports: `import py.<module>` and `from
  py.<module> import`, a bare import of a Python module an error suggesting `py.<name>`, and the
  tests, guide and reference moved to it
  (adr:0029-foreign-modules-are-imported-by-origin-and-their-interfaces-generated)
- [x] 2.1 (Unit) Write a proposed ADR on a dynamic Python value for the names a stub cannot type,
  amending adr:0012: opaque, left only through a conversion that runs the boundary checks, kept
  out of `lotml build` and of Python calling LotML; the owner accepts it before 2.2
- [x] 2.2 (Unit) Write and build the spec python-object
  _Depends 2.1_
- [x] 2.3 (Unit) Write a proposed ADR on binding a module at check time, extending the ADR of
  bind-sources 2.1 and saying whether python-via-uv 2.1 stands: a stub read in Rust as data, never
  the module imported or run, a typeshed embedded at a pinned version, the interface written as a
  lock recording its source; the owner accepts it before 2.4
- [ ] 2.4 (Unit) Write and build the spec bind-on-import, every module of the binding-coverage
  corpus imported as `py.<module>` under `lotml run` with no `lotml bind`, after bind-sources 1.1
  _Depends 1.2, 1.3, 2.2, 2.3, 2.5_
- [x] 2.5 (Unit) Write a proposed ADR on `lotml run` installing a project's dependencies through uv
  from its `pyproject.toml` and `uv.lock`, amending adr:0026: the lock's versions are the ones
  bound, and `uv.toml` and `.python-version` stay ignored; the owner accepts it before 2.4
- [ ] 3.1 (Unit) Write and build the spec python-classes, the coverage report run before and after
  _Depends 2.4_
- [ ] 3.2 (Unit) Write and build the spec python-overloads, the coverage report run before and after
  _Depends 2.4_
- [ ] 3.3 (Unit) Write and build the spec python-generics, the coverage report run before and after
  _Depends 3.1_
- [ ] 4.1 (Unit) Write the wiki page on calling Python from LotML through `py.<module>`
  _Depends 2.4_
- [x] 2.6 (Unit) Write and build the spec rust-binder: `lotml bind` reads a stub with
      `ruff_python_parser` in Rust and needs no Python, with typeshed's `stdlib`
      vendored as text at a pinned commit and embedded in lotml, writing the interfaces
      the Python binder writes
  _Depends 2.3_
  _Reason the owner split 2.4 into three specs on 2026-10-08, one PR each; this is adr:0032's parser and embedded typeshed_
- [ ] 2.7 (Unit) Write and build the spec python-dependencies: `lotml run` and `lotml
      test` install a project's Python dependencies from its `uv.lock` as adr:0033 has
      it, so 2.4 runs the binding-coverage corpus's PyPI modules
  _Depends 2.5_
  _Reason the owner split 2.4 into three specs on 2026-10-08, one PR each; this is adr:0033's install_

## Done when

- `lotml run` runs a program importing each module of the binding-coverage corpus as `py.<module>`, with no
  `bindings/` directory written by hand or by `lotml bind`.
- The binding coverage report shows the typed share before and after each of 3.1, 3.2 and 3.3.
- The ADRs from 2.1, 2.3 and 2.5 are `accepted`, and `scc validate` exits 0.
