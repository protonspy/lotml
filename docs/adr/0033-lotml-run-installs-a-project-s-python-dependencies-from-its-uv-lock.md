---
status: accepted
---

# 0033 · lotml run installs a project's Python dependencies from its uv.lock

## Context

adr:0026-lotml-ships-uv-and-runs-python-3-14-by-default ships uv, resolves the interpreter, and
leaves "declaring a project's Python dependencies so `lotml run` passes them to uv" to a later
decision. A program that imports `py.requests` needs `requests` installed where it runs, and its
interface bound from the version installed (adr:0032-a-python-module-is-bound-at-check-time-from-stubs-read-in-rust).
Python's own tooling has settled on `pyproject.toml` for declaring dependencies and on a lock
file for pinning them; uv reads both, and writes `uv.lock` with a hash for every artifact.

adr:0026 confines every uv call: run from lotml's cache, the project's `uv.toml`, `[tool.uv]` and
`.python-version` ignored, so a project cannot redirect a download.

## Decision

A project declares its Python dependencies in `pyproject.toml`, `[project] dependencies`, and pins
them in `uv.lock`, both as Python's tooling writes them. lotml reads them and adds no format of its
own.

- **`lotml run` and `lotml test` install from the lock.** When the project root holds a `uv.lock`,
  they create an environment from it in lotml's cache, keyed by the lock's hash, with the interpreter
  adr:0026 resolves: `uv sync --locked` from the lock alone, every artifact's hash checked, wheels
  only (`--no-build`, so no package's build code runs), the project itself not installed. The
  program runs in that environment.
- **The lock's versions are the ones bound.** adr:0032 reads the stubs of the `py.` modules from
  that environment, so the interface a program is checked against is the version it runs with.
- **No lock, no install.** A `pyproject.toml` with dependencies and no `uv.lock` is reported with
  the command that writes the lock, `uv lock`; lotml never resolves versions itself, which would
  make two runs install two sets.
- **Still confined as adr:0026 has it.** `uv.toml`, `[tool.uv]` and `.python-version` stay ignored;
  the offline switch forbids the install, and an environment already in the cache is used offline.
- **The lock is project input, not a source of indexes.** A package is installed only from a
  registry source on an allowed index, PyPI unless the user's own lotml configuration names
  another; a lock naming a direct URL, a git, path or directory source, or an editable one, is
  refused, and uv runs with its sources ignored. A hash in the lock proves a wheel is the one the
  lock names, not that its index is one to trust.
- **`run` and `test` need a trusted project**, as they always have: they execute its code, and a
  wheel's `.pth` file runs at the interpreter's start. Only `check` stays inert on an untrusted
  project, and it never installs.
- **`lotml check` never installs.** It binds from an environment already made for the lock, or,
  without one, reports each `py.` module it cannot bind and why, and binds the standard library
  from the embedded typeshed alone.
- **The MCP server, the grader and the harness never install**, as adr:0026 has them never download.

Rejected:
- `requirements.txt`: no hashes by default, and no place for the interpreter or the project.
- A `[tool.lotml]` table of dependencies: a second format for what `pyproject.toml` already says.
- Resolving at run time without a lock: a dependency's new release changes what a program runs and
  how it is bound, between two runs of the same commit.
- Building source distributions: a package's build code would run on install.

## Consequences

- Amended on 2026-10-08, before it was merged, with the constraints its first security review
  asked for.
- A program's Python dependencies are reproducible from its repository: `pyproject.toml`, `uv.lock`
  and the lotml version.
- The first `lotml run` of a project downloads its wheels into lotml's cache, as adr:0026's first
  run downloads Python; a package with no wheel for the platform cannot be used until it has one.
- `lotml check` on a fresh clone binds only what the embedded typeshed covers until `lotml run` or
  `lotml test` has made the environment, and says so per module.
- uv becomes the one tool that reads the project's lock; lotml depends on its lock format staying
  readable by the uv it pins.
