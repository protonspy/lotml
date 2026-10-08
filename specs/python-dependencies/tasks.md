# Python dependencies — tasks

## 1 · The lock

- [x] 1.1 (Unit) Read a project's `uv.lock` with the `toml` crate and refuse a source other than PyPI's registry, save the project's own entry, and an artifact off PyPI's file host or without a hash, naming the package; record `toml` in `docs/stack.md` — R2.1, R2.2
- [x] 1.2 (Unit) Report a `pyproject.toml` that declares dependencies with no `uv.lock`, naming `uv lock` — R1.4

## 2 · The environment

- [x] 2.1 (Unit) Key the environment by the lock, the `pyproject.toml` and the base interpreter, and make it in lotml's cache with one confined `uv sync` from copies of the two files, marked complete only once uv succeeds — R1.1, R1.2, R1.3
  _Depends 1.1_
- [ ] 2.2 (Unit) Run `lotml run` and `lotml test` in the lock's environment over a base interpreter that is never the project's `.venv`, using a complete environment as it is and making none where downloads are off — R1.1, R1.3, R3.1, R3.2
  _Depends 1.2, 2.1_
- [ ] 2.3 (Unit) Describe a project's Python dependencies in the wiki page on transpilation — R1.1
  _Depends 2.2_
