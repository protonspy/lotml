# Bind on import — tasks

## 1 · Binding on import

- [x] 1.1 (Unit) Move `lotml bind`'s stub resolution into one function the compiler shares: the embedded typeshed for the standard library, else the packages of the lock's environment, never `VIRTUAL_ENV` or `.venv`, answering the stub's text, source and SHA-256 or why there is none — R1.2
- [x] 1.2 (Unit) Find the lock's environment without running Python: a completed environment under `python-environments/` whose copied `uv.lock` and `pyproject.toml` equal the project's, newest first — R1.2
  _Depends 1.1_
- [x] 1.3 (Unit) Generate, in `files::interfaces_for`, the interface of each `py.` module a file imports that no bindings file covers, and key `InterfaceCache` by the imports too, so `check`, `run`, `test`, the language server and the MCP server all bind on import — R1.1
  _Depends 1.2_
- [x] 1.4 (Unit) Make the lock's environment in `run` and `test` before their interfaces are collected — R1.3
  _Depends 1.3_
- [x] 1.5 (Unit) Report a `py.` import bound from no stub, or from a stub that binds nothing, with the reason and "`lotml bind <module>` tells why" — R1.4
  _Depends 1.3_
- [x] 1.6 (Unit) Offer the fix that writes `py.<name>` on a bare import the embedded typeshed covers — R1.5
  _Depends 1.3_
- [x] 1.7 (Unit) Warn at an import whose bindings file shadows the interface the compiler would generate — R1.6
  _Depends 1.3_

## 2 · The lock and the cache

- [x] 2.1 (Unit) Keep generated interfaces in the user's private `interfaces/` cache, keyed by the stub's SHA-256 and lotml's version, written whole — R3.1
  _Depends 1.3_
- [x] 2.2 (Unit) Write `lotml.lock` with `lotml bind --lock`: each `py.` module the project's programs import, its source, its stub's and its interface's SHA-256, sorted, names checked — R2.1
  _Depends 1.1_
- [ ] 2.3 (Unit) Warn at an import whose stub's hash differs from the lock's, binding from the stub found, and use a cached interface for a module the lock names only when its hash is the lock's — R2.2, R3.1
  _Depends 2.1, 2.2_
- [ ] 2.4 (Unit) Add `lotml check --locked`, failing on a missing lock, a differing stub or an import the lock lacks — R2.3
  _Depends 2.3_

## 3 · The corpus

- [ ] 3.1 (Unit) Bind the binding coverage report's standard-library modules through `lotml bind <module>` without `--stub`, and record typeshed's commit in the report — R4.2
  _Depends 1.1_
- [ ] 3.2 (Unit) Run each module of the binding coverage corpus imported as `py.<module>` under `lotml run` with no `bindings/`, the PyPI ones in a scratch project that locks them, and report each that fails — R4.1
  _Depends 1.4, 3.1_
