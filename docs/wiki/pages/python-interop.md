# Python interop

How a LotML program calls Python: `import py.<module>` and the module is there, typed as far as
its stub says and opaque past that, every call able to fail. The binder's mechanics are in
[[transpilation-strategy]]; this page is the whole picture from the program's side, and what is
still out of reach.

## Importing a module

A program names a Python module by its origin, `import py.textwrap` then `py.textwrap.dedent(s)?`,
or `from py.textwrap import dedent`; a bare `import textwrap` is a LotML module
(adr:0029-foreign-modules-are-imported-by-origin-and-their-interfaces-generated). No `lotml bind`
comes first: `check`, `run`, `test` and both servers generate the module's interface on import
(specs/bind-on-import).

- **The stub is read, never run.** The binder is Ruff's parser in Rust
  (adr:0032-a-python-module-is-bound-at-check-time-from-stubs-read-in-rust). A standard-library
  stub is typeshed's, embedded in lotml at a pinned commit; any other is found in the environment
  made from the project's `uv.lock`, and nowhere else
  (adr:0033-lotml-run-installs-a-project-s-python-dependencies-from-its-uv-lock).
- **What was bound is recorded.** `lotml bind --lock` writes `lotml.lock`: each module's source and
  the hashes of its stub, the stubs it re-exports from, and its interface. A stub that differs from
  the lock is bound all the same and warned (E0225); `lotml check --locked` fails on it, for CI.
- **A file can stand in.** `bindings/py.<module>.lotmli`, written by `lotml bind <module>` or by
  hand, is used instead of the generated interface, and warned where a stub would give one
  (E0224).
- **Python target only.** A native program (`lotml build`) that imports a Python module is refused
  (E0401).

## What a value of each kind becomes

| The stub says | The program sees | Decided by |
|---|---|---|
| `int`, `str`, `list[str]`, `dict[str, int]`, `X \| None` | the LotML type, copied across | adr:0012-python-interop-through-checked-boundaries-and-interface-files |
| a union, `Any`, a callable, a class it does not define | `PyObject`: passed back, or converted with `o.value()`, typed | adr:0031-a-python-name-no-stub-types-crosses-as-an-opaque-python-value |
| a class it defines | a nominal handle, `date`, reached only through its declared members | adr:0034-a-python-class-crosses-as-a-nominal-handle-with-its-declared-members |
| `@overload` | one signature per overload; a call takes the first its arguments fit | adr:0035-a-python-overload-crosses-as-ordered-signatures-chosen-at-the-call |
| a `TypeVar`, a generic class | a type parameter inferred per call; `Pattern[str]` | adr:0036-a-python-type-variable-crosses-as-a-type-parameter-checked-where-a-value-crosses |
| a name re-exported from a submodule | the module's own, bound from where it is defined | specs/python-reexports |

Everything Python is a call, so everything returns `T ! PyError`: a function, a constructor, a
method, an attribute read. An exception becomes `Err(PyError(kind, message))`, and so does a value
Python returns that is not the type declared: the boundary checks every value that crosses, which
is what makes a stub's claim safe to believe. A handle's type arguments are not checked when the
handle crosses, since Python erased them; each value read through it is. A handle is never printed,
compared, hashed or assigned to, only asked whether it is there (`m is None`); an overloaded
function is called, never passed (E0226).

## How much of Python that reaches

The binding coverage report (specs/binding-coverage) measures a fixed corpus of 36 modules, half
the standard library and half PyPI, label by label beside the earlier ones. After re-exports, 30.9%
of their public names are bound typed and 66.4% reachable at all, `PyObject` included; every one of
the 36 imports and runs under `lotml run` with no `lotml bind`
(`harness/results/bind-on-import.md`). Each step's share is in `harness/results/binding-coverage.md`.

## What is still out

- **Variadic parameters.** `*args` and `**kwargs` are left to Python, so `os.path.join(a, b)`
  takes one argument.
- **Operators.** Dunder methods are listed, not bound: a handle has no `+` or `[]`.
- **Protocols, `ParamSpec`, `TypeVarTuple`.** A protocol is a shape, not a class, and is left out;
  the other two are `PyObject`.
- **A submodule re-exported as a module**, `from os import path as path`: import `py.os.path`.
- **Inheritance from a generic base.** Its members take fresh type arguments, and its constructor
  is not inherited (`docs/notes.md`).
- **Python calling native LotML.** Python calls the Python target's modules today; loading a
  `lotml build --shared` library from CPython is proposed in
  adr:0037-cpython-loads-a-shared-lotml-library-through-a-generated-checked-ctypes-module.
- **Limits a hostile stub cannot pass.** 8 MiB, a nesting of 100, 20 000 tokens a line, 64
  overloads and 16 type parameters a name, 4 re-exporting modules deep and 256 in all.
