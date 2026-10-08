---
autonomy: auto
ci: wait
branch: feat/python-dependencies
delivery: in-progress
---

# Python dependencies — requirements

## Purpose

A program that imports a PyPI package runs with the versions its project pinned: `lotml run` and
`lotml test` make an environment from the project's `uv.lock` and run the program in it, as
adr:0033-lotml-run-installs-a-project-s-python-dependencies-from-its-uv-lock decides, so the version
a program runs with is the one the spec bind-on-import binds it against.

## R1 · Running in the lock's environment

- **R1.1** When `lotml run` or `lotml test` runs a program whose project root holds a `uv.lock`, the command shall run it in an environment made from that lock in lotml's cache, keyed by the lock, the `pyproject.toml` and the interpreter.
- **R1.2** The command shall make the environment with `uv sync` from a copy of the project's `pyproject.toml` and `uv.lock` alone, installing the lock as it stands without resolving it again, wheels only, leaving the project itself uninstalled, and ignoring uv's configuration.
- **R1.3** When the environment for that key is already complete, the command shall use it without running uv, offline included.
- **R1.4** If the project root holds a `pyproject.toml` that declares dependencies and no `uv.lock`, then the command shall report it and name the command that writes the lock, `uv lock`.

## R2 · What a lock may name

- **R2.1** If the lock names a package from a source other than PyPI's registry, then the command shall refuse the lock, naming the package and its source, the project's own entry excepted.
- **R2.3** If the project's root is a drive's root, or a directory another user may write, then the command shall ignore its `uv.lock` and `pyproject.toml` and say so.
- **R2.2** If an artifact the lock names lies anywhere but PyPI's file host or carries no hash, then the command shall refuse the lock, naming the artifact.

## R3 · Where nothing is installed

- **R3.1** While the offline switch is on, the command shall install nothing, and if the environment is missing, shall report that and how to make it.
- **R3.2** When the MCP server, the grader or the harness runs a program, the lotml binary shall install nothing.

## Out of scope

- Binding the `py.` modules a program imports against that environment: `specs/bind-on-import/`.
- An index other than PyPI: lotml has no configuration of its own to name one yet.
- `lotml check`, which runs no Python and so never installs.
