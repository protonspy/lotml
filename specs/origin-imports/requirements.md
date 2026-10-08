---
autonomy: auto
ci: wait
branch: feat/python-compat-group-1
delivery: in-progress
---

# Origin imports — requirements

## Purpose

A program says where a module comes from: `import py.random` reaches CPython's `random`, a bare
`import` names a LotML module only, and `c.<library>` keeps naming a C library
(adr:0029-foreign-modules-are-imported-by-origin-and-their-interfaces-generated). The reader of an
import knows which rules its calls follow, `T ! PyError` for Python, before reading the calls.

## R1 · Importing a Python module by origin

- **R1.1** When a program writes `import py.<module>` and an interface of `py.<module>` is available, the checker shall bring the module into scope as `py.<module>`, each function of the interface called as `py.<module>.<function>` and returning `T ! PyError`.
- **R1.2** When a program writes `from py.<module> import <name>` and the interface of `py.<module>` declares `<name>`, the checker shall bring `<name>` into scope as that function.
- **R1.3** The interface of `py.<module>` shall be the file `bindings/py.<module>.lotmli`, and `lotml bind <module>` shall write it there.
- **R1.4** When a program importing `py.<module>` runs on the Python target, the compiled program shall call the functions of CPython's module `<module>`.

## R2 · A bare import

- **R2.1** If a program imports a module by a bare name that is no LotML module, then the checker shall report E0216, and, when an interface of `py.<name>` is available, give the fix that writes `py.<name>`.
- **R2.2** If an import names the interface of a Python module whose file `bindings/<module>.lotmli` carries no origin, then the checker shall report E0216 there and say to rename the file `bindings/py.<module>.lotmli`.

## R3 · What a reader is taught

- **R3.1** The repository shall import every Python module as `py.<module>` in its tests, its agent guide and `reference/lotml.md`.

## Out of scope

- Generating an interface on import, with no `bindings/` file: `specs/bind-on-import/`.
- A Python name the interface cannot type: `specs/python-object/`.
- `c.<library>`: its name and its hand-written interface stay as adr:0013 left them.
