---
autonomy: auto
ci: wait
pr: per-group
merge: manual
status: approved
checksum: c7bdb0230c248df0d785164ac6eaee40108fbfbde7e7c7b09a8cec2632a7f1bd
---

# Facades

LotML modules that import one another, LotML packages as wheels on PyPI, and the facades the
community writes with them: a LotML-shaped API over one foreign library
(adr:0038-a-facade-is-a-lotml-package-published-on-pypi-as-a-wheel). Two reference facades prove
the format: redis for a stateful client, FastAPI for a framework that calls the program back.

## Why

`import py.<module>` reaches every Python library, but with the library's Python shape: handles,
`PyObject` and one `PyError`. A connection used through `with`, a framework that calls a handler,
or a dozen exception classes have no LotML form, and the syntax is frozen. A facade gives one
library an API designed for LotML, and the community can publish one without a compiler release.
LotML cannot yet import one LotML file from another (`MODULES` is `math` alone), so modules come
first, on both targets. Done when a project that locks `lotml-redis` or `lotml-fastapi` imports it
bare under `lotml run`, a pure LotML package builds natively, both facades are on PyPI and in the
index, and the index's CI runs their tests against the real libraries.

## Paths

- `compiler/crates/lotml-check/src/program.rs` — where an import resolves to a module
- `compiler/crates/lotml-db/` — the inputs the incremental check reads
- `compiler/crates/lotml-bind/` and `compiler/crates/lotml/src/files.rs` — the project's
  environment and what is found in it
- `compiler/crates/lotml-py/` and `compiler/crates/lotml-llvm/` — linking modules on each target
- `facades/` — the reference facades
- `index/packages.toml` — the package index, the allowlist lotml fetches
- `docs/wiki/pages/` — writing a facade

## References

- `specs/lotml-modules/` — `import <name>` reaching another LotML file of the project, checked and
  run on the Python target
- `specs/llvm-modules/` — a program of several LotML modules built into one native executable
- `specs/lotml-packages/` — a wheel carrying LotML source, found in the project's environment as a
  second module root
- `specs/package-index/` — the allowlist: fetched, cached, failing closed, checked against `uv.lock`
  before installing and against the environment before any Python starts, and a project's own list
- `specs/redis-facade/` — `lotml-redis`, the reference facade over a stateful client
- `specs/python-callbacks/` — a LotML function passed to Python, every argument checked when
  Python calls it
- `specs/fastapi-facade/` — `lotml-fastapi`, the reference facade over a framework that calls back
- `specs/agent-guide/` — the guide 2.4 amends, on when a program imports a facade
- `plans/python-compatibility.md` — the typed `py.` binding a facade is written over

## Out of scope

- A numpy facade: numpy is value-shaped and needs operators on handles first
  (specs/python-classes R2.5), so it is a plan of its own after that.
- A native implementation of a facade: `lotml build` keeps refusing a `py.` import (E0401).
- A facade over `c.`: a package's own headers are not a search root of adr:0029.
- `async` handlers, decorators, and subclassing a Python class.
- A registry or resolver of LotML's own, and packages from anywhere but PyPI (adr:0033).

## Tasks

- [ ] 1.1 (Unit) Write a proposed ADR on importing a LotML module from another file: how
  `import <name>` finds a file of the project, what crosses (records, sums, traits, functions) and
  with what nominal identity, the cycle rule, how the Python target and the LLVM target link the
  files, and what `check`, `test`, `fmt` and the language server read; amends adr:0029; the owner
  accepts it before 1.2
- [ ] 1.2 (Unit) Write and build the spec lotml-modules
  _Depends 1.1_
- [ ] 1.3 (Unit) Write and build the spec llvm-modules
  _Depends 1.2_
- [x] 2.1 (Unit) Write the proposed adr:0038-a-facade-is-a-lotml-package-published-on-pypi-as-a-wheel,
  with `package` and `facade` in the glossary; the owner accepts it before 2.2
- [ ] 2.2 (Unit) Write and build the spec lotml-packages: a wheel carrying LotML source and its
  lotml range, its manifest read from the wheel's files, found in the environment from `uv.lock`
  as a second module root for direct dependencies only, read from lotml's cache environment and
  never a project `.venv`, under adr:0029's confinement and size caps, the package's source hash
  in `lotml.lock`, imported bare, embedded names unclaimable, two candidates for one name an error,
  no `py.` suggested for a name a package provides, a missing environment reported with the
  command that makes it, a `lotml check` that runs no Python, and a pure package built natively
  _Depends 1.3, 2.1_
- [ ] 2.3 (Unit) Write and build the spec redis-facade: `lotml-redis` under `facades/redis/`,
  connecting, reading, writing, expiring and pipelining keys, redis' exceptions mapped to error
  values, its Python module stubbed, its own tests run under `lotml test` before the wheel exists,
  against a real redis in CI on Linux only, recorded as a ceiling note
  _Depends 2.2_
- [ ] 2.4 (Unit) Say in the agent guide when a program imports a facade and when `py.`
  _Depends 2.3_
- [ ] 2.5 (Unit) Add redis, fastapi, uvicorn, httpx and the wheel build and publish tooling to
  `docs/stack.md`, and extend the ruff paths in `.claude/rules/project.md` and the CI lint to
  `facades/`
  _Depends 2.3_
- [ ] 2.6 (Unit) Publish `lotml-redis` to PyPI from a facade release workflow with its own tag
  scheme, through trusted publishing with a PEP 740 attestation, from a protected environment
  whose publish job alone holds the OIDC token; the owner registers the pending publisher and
  approves the first upload
  _Depends 2.3_
- [ ] 2.7 (Unit) Write and build the spec package-index: `index/packages.toml` with `lotml-redis`
  as its first entry, lotml fetching and caching it and failing closed, a lock or an environment
  holding an unlisted LotML package refused, revocation, the project's own list warned, and the CI
  job that verifies each new entry's attestation and hash and runs its tests on `pull_request`,
  read-only, with no secrets or OIDC; and a wiki page on writing a facade drawn from 2.3
  _Depends 2.2, 2.6_
- [ ] 3.1 (Unit) Write a proposed ADR on a LotML function crossing to Python as a callable: each
  argument Python passes checked against the LotML signature and copied, a wrong one raised in
  Python, what a `T ! E` return and a panic become in Python, which thread runs it, and how a
  stub's `Callable` is typed instead of `PyObject`; amends adr:0012 and adr:0031; the owner
  accepts it before 3.2
- [ ] 3.2 (Unit) Write and build the spec python-callbacks
  _Depends 3.1_
- [ ] 3.3 (Unit) Write and build the spec fastapi-facade: `lotml-fastapi` under
  `facades/fastapi/`, routes registered by a call with LotML handlers, path, query and JSON body
  decoded into LotML values and checked, responses encoded, served by uvicorn, tested through
  FastAPI's `TestClient`, with request size and nesting capped before the boundary, handlers
  serialized into the runtime, error and panic text kept out of responses, and uvicorn bound to
  `127.0.0.1` unless asked
  _Depends 2.7, 3.2_
- [ ] 3.4 (Unit) Publish `lotml-fastapi` as 2.6 published `lotml-redis`, then add it to the index
  _Depends 3.3_

## Done when

- A program of two LotML files, one importing the other, passes `lotml check`, runs under
  `lotml run` and builds with `lotml build`.
- A project whose `uv.lock` names `lotml-redis` runs `import redis` under `lotml run`, and once that
  run has made the environment, `lotml check` of it passes with no Python on `PATH`.
- A project depending on a pure LotML package builds it into a native executable.
- A LotML program serves a FastAPI route whose JSON body is checked against a LotML record, and
  its test passes through `TestClient`.
- `lotml-redis` and `lotml-fastapi` are on PyPI and in `index/packages.toml`, and the index's CI
  job is green.
- A project whose lock names a LotML package missing from the index, or listed with another hash,
  installs nothing and is told which distribution was refused.
- adr:0038 and the module record are accepted, or superseded by the records that replaced them.
