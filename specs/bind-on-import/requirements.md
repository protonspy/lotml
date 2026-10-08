---
autonomy: auto
ci: wait
branch: feat/bind-on-import
delivery: in-progress
---

# Bind on import — requirements

## Purpose

A program imports a Python module by its origin, `import py.random`, and needs no `lotml bind`
first: the compiler reads the module's stub and generates its interface itself, running no Python,
as adr:0029-foreign-modules-are-imported-by-origin-and-their-interfaces-generated and
adr:0032-a-python-module-is-bound-at-check-time-from-stubs-read-in-rust decide, and `lotml.lock`
records what was bound so that two machines bind alike.

## R1 · Binding on import

- **R1.1** When a program imports `py.<module>` and no `bindings/py.<module>.lotmli` applies to it, the compiler shall generate the module's interface from its stub as `lotml bind` writes it, in `lotml check`, `run`, `test`, the language server and the MCP server, running no Python.
- **R1.2** The compiler, `lotml bind` among its commands, shall read a standard-library module's stub from the typeshed lotml carries, and any other module's from the environment made from the project's `uv.lock` and from no other environment, `VIRTUAL_ENV` and `.venv` included.
- **R1.3** When `lotml run` or `lotml test` runs a project holding a `uv.lock`, the command shall make the lock's environment before it binds the program's imports.
- **R1.4** If the compiler finds no stub for an imported module, or binds nothing from one, then the checker shall report the import and say that `lotml bind <module>` tells why.
- **R1.5** When a bare `import <name>` names no LotML module and the typeshed lotml carries has a stub for `<name>`, the checker shall offer the fix that writes `py.<name>`.
- **R1.6** When a `bindings/py.<module>.lotmli` applies to an import for which the compiler would generate an interface, the checker shall warn at the import that the file shadows the generated interface.

## R2 · The lock

- **R2.1** When `lotml bind --lock` runs in a project, the command shall write `lotml.lock` at the project root, recording for each `py.` module its programs import the source of its stub, the stub's hash and the interface's hash.
- **R2.2** If the compiler binds a module `lotml.lock` names from a stub whose hash differs from the lock's, then the checker shall warn at the import, and bind from the stub found.
- **R2.3** Where `lotml check --locked` is run, the command shall fail when the lock is missing, names a stub that differs, or lacks a module a program imports.

## R3 · The cache

- **R3.1** The compiler shall keep each generated interface in a directory of the user's own, keyed by the stub's hash and lotml's version, and use an entry only when its hash is the one the lock records, where the lock names the module.

## R4 · The corpus

- **R4.1** The harness shall run each module of the binding coverage corpus imported as `py.<module>` under `lotml run`, with no `lotml bind`, the PyPI ones in a project that locks them, and report each that fails.
- **R4.2** The binding coverage report shall bind standard-library modules from the typeshed lotml carries, and record its commit.

## Out of scope

- Typing what the binder leaves unbound — classes, overloads, generics: their own specs.
- A C library's interface from its header (`c.<library>`), the other half of adr:0029.
