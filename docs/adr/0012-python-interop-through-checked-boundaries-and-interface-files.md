---
status: accepted
---

# 0012 · Python interop through checked boundaries and interface files

## Context

Phase 2's gate asks for incremental adoption: Python calling lotml, and lotml calling Python
(R14, R27; [[transpilation-strategy]]). The Python backend already compiles each file to a module
that Python can import, but its functions trust their arguments. If a caller passes a `str`
where an `int` is declared, the error comes from deep inside the function. If the caller passes
an `int` past 64 bits, the overflow trap is skipped. If the caller passes a list and keeps it,
the caller sees the aliasing that value semantics rules out. In the other direction, a lotml
file could import only `math`, lotml's own module.

typeshed's stubs, which mypy, pyright, PyCharm, Pyrefly and ty all read, describe Python's
modules with types. No language generates bindings from them yet. The stubs declare no
exceptions, so every call into Python can fail, and a stub can be wrong about what a function
returns. Each lotml file is a module of its own, and there is no syntax for declaring a foreign
function. Calling `lotml check` must stay fast and must not run Python.

## Decision

Both directions cross a checked boundary, and Python's modules reach lotml through generated
interface files.

- **Python calling lotml.** Each public function of a compiled module is wrapped. The wrapper
  checks the arguments against the lotml signature: the integer ranges, `int` widened to `f64`,
  the element types, and records and variants of this module. It copies the arguments so the
  caller's values are never shared. A function returning `T ! E` returns `T` or raises
  `lotml_rt.LotmlError` carrying the error, which is the exception a Python caller expects.
  Inside the module, lotml calls lotml unwrapped. `lotml build` also writes a `.pyi` next to
  each module, so Python's type checkers see the lotml types.
- **lotml calling Python.** `lotml bind <module> --stub <file.pyi>` reads a stub with Python's
  own parser and writes `bindings/<module>.lotmli`. That file is a lotml interface: one
  signature per function, with no body, each returning `T ! PyError`. Functions whose types
  lotml cannot express are listed in comments with the reason: unions other than `X | None`,
  `Any`, callables, classes, and overloads. A parameter with Python's own default is written
  `= todo()`. `import m` resolves to the nearest `bindings/m.lotmli` in the importing file's
  directory or one of its ancestors. At run time the call goes through the runtime, which turns
  any exception into `Err(PyError(kind, message))` and checks the returned value against the
  declared type. A stub that lied becomes an error, not a wrong value.

Rejected:
- Reading `.pyi` files at check time. That would need a Python parser in the compiler, or
  running Python on every check.
- Hand-written declarations, as Erg and Codon require.
- Treating Python values as dynamic, as Mojo does. That gives up the types a stub already has.
- A new `extern` syntax in ordinary files. An interface is a separate kind of file, so a
  program file cannot declare a function without writing its body.

## Consequences

- An interface file is generated and checked in. When typeshed changes, it is regenerated,
  and the diff of the interface shows how Python's API changed.
- `PyError` is a prelude record, `PyError(kind: str, message: str)`, so a program can match on
  it.
- Crossing a boundary costs a check and a copy, proportional to the size of the value. That is
  the price of the integer range, the declared types and value semantics holding at the edge.
- Bindings cover module-level functions over builtin types. Python classes and callbacks into
  lotml are left for later, recorded as ceilings rather than half-supported.
