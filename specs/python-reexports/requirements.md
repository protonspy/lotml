---
autonomy: auto
ci: wait
branch: feat/python-reexports
delivery: in-progress
---

# Python re-exports — requirements

## Purpose

Many modules define little and re-export much: `certifi` is `from .core import contents, where`,
`os.path` is `from posixpath import *`, `dateutil.parser` re-exports `parse` from `._parser`. The
binder reads one stub, so such a module binds no function and its import fails
(`harness/results/bind-on-import.md`, n-0117). This binds a re-exported name as the module's own,
read from the stub of the module it comes from, for a program calling Python through
`py.<module>`.

## R1 · Binding

- **R1.1** When a stub imports a name from another module and re-exports it, through `__all__` or as `name as name`, the binder shall bind the name as the stub's own from the stub of the module it comes from.
- **R1.2** When a stub imports `*` from a module, the binder shall bind that module's public names as the stub's own.
- **R1.3** When the module a name comes from re-exports it in turn, the binder shall follow it to the module that defines it, through at most 4 modules.
- **R1.4** When a stub re-exports a name under another name, the binder shall bind it under the name the stub gives it.
- **R1.5** If a module a stub re-exports from has no stub, then the binder shall list each name it would have given as not bound, with the reason.
- **R1.6** If the stubs a module re-exports from come to more than 8 MiB, then the binder shall refuse to bind the module.

## R2 · Finding stubs

- **R2.1** When the compiler finds a module's stub, the compiler shall find the stubs the module re-exports from as it finds the module's own.
- **R2.2** When `lotml bind` is given a stub, the compiler shall read a module of the stub's package from beside the stub, and any other module as it finds a module's stub.
- **R2.3** The compiler shall record in `lotml.lock`, and key its cache of interfaces by, the stubs a module re-exports from as well as its own.

## R3 · Coverage

- **R3.1** The binding coverage report shall keep the measurement before this spec and the one after under two labels.
- **R3.2** The bind-on-import report shall be measured again after this spec.

## Out of scope

- A submodule re-exported as a module, `from os import path as path`: a module is imported by its own name.
- One Python class reached through two modules is two LotML types, as adr:0034 makes a class by
  the module that names it.
- A module-level `__getattr__`, which names nothing a stub can bind.
