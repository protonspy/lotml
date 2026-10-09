---
autonomy: auto
ci: wait
branch: feat/rust-binder
delivery: merged
pr: 55
---

# Rust binder — requirements

## Purpose

`lotml bind` writes a Python module's interface from its stub without running Python: the stub is
read by a parser in lotml itself, and the standard library's stubs come from a copy of typeshed
lotml carries, as adr:0032-a-python-module-is-bound-at-check-time-from-stubs-read-in-rust decides.
It is the binder the spec bind-on-import then runs at check time.

## R1 · Reading a stub

- **R1.1** The `lotml bind` command shall read a stub with a Python parser built into lotml and run no Python interpreter.
- **R1.2** The `lotml bind` command shall write from a stub the interface the Python binder wrote from it, save R1.3's escapes.
- **R1.3** The `lotml bind` command shall write every string it takes from a stub into an interface through one escape function, so that no control character reaches the interface unescaped.
- **R1.4** If a stub is larger than 8 MiB, nests deeper than lotml's limit, or does not parse, then the `lotml bind` command shall bind nothing from it and report which limit it passed or where it fails to parse.

## R2 · The standard library's stubs

- **R2.1** The lotml binary shall carry typeshed's `stdlib` stubs from a typeshed commit the repository pins, with typeshed's licence.
- **R2.2** When no stub is given, the `lotml bind` command shall read a standard-library module's stub from the stubs lotml carries.
- **R2.3** If typeshed's `VERSIONS` says a standard-library module is absent from CPython 3.14, then the `lotml bind` command shall report that and bind nothing.
- **R2.4** If a module's top-level name is a standard-library one, then the `lotml bind` command shall never read it from the project's packages.

## R3 · Measuring it

- **R3.1** The binding coverage report shall count the names the `lotml bind` command binds, by running it.

## Out of scope

- Binding when a program imports `py.<module>`, `lotml.lock` and the cache of generated
  interfaces: `specs/bind-on-import/`.
- Installing a project's dependencies: `specs/python-dependencies/`.
- Typing what the Python binder left unbound — classes, overloads, generics: their own specs.
