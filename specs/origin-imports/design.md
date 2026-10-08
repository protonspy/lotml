# Origin imports — design

## The interface's name is the import's

Serves R1.1, R1.2, R1.3, R2.2.

An interface is keyed by its file's stem, as today: `bindings/py.textwrap.lotmli` is the
interface of `py.textwrap`, as `bindings/c.m.lotmli` is that of `c.m`. `Program::import` reads a
path starting `py.` as a Python module, through the interface of that exact name; `lotml bind
<module>` writes `bindings/py.<module>.lotmli`, a `py.` given with the name taken off first. An
interface whose name has no origin is no longer one a program can import: a bare import naming it
is E0216 with the rename to make (R2.2). Keying by the full name keeps a Python `json` and a
future LotML `json` apart.

## `py` in an expression

Serves R1.1.

`import py.textwrap` brings in the module path `py.textwrap`, as Python's `import os.path` does.
A name that is the first segment of an imported module path, `py`, has the type of a package,
`Ty::Module("py")`; an attribute of a package is the module or the package it names,
`Ty::Module("py.textwrap")`, and an attribute of a module with an interface is its function, as
for a bare module today. Neither a package nor a module is a value: lowering reaches the call
through the module's type and never evaluates the path.

## The Python target

Serves R1.4.

The runtime's `foreign(module, function, returns)` imports the module named, so the backend writes
the module's name with `py.` taken off. The native target keeps refusing a Python import
(adr:0025), whatever its prefix.

## A bare import

Serves R2.1.

E0216 for a bare name now says that a Python module is imported as `py.<name>`, and when the
interface of `py.<name>` is available, it lists `py.<name>` among the alternatives with a
machine-applicable fix over the module's name. `math` stays LotML's own module.
