# Python re-exports — design

## What changes

Serves R1.1, R1.2, R1.3, R1.4, R1.5, R1.6, R2.1, R2.2, R2.3, R3.1, R3.2.

**The binder stays a function of text.** It does not look for files; it is given them.
`lotml-bind` gains `sources(package, text, read)`. Given the stub's package, it walks the stub's
re-exporting imports (`from M import n as n`, a name `__all__` lists, `from M import *`), and from
there each source's, breadth first. It makes a relative module absolute against the package, asks
`read` for each module once, and stops at depth 4 or past 8 MiB in all. It returns each `Part`
read: the module, its package and its text. `interface(module, stub, said)` becomes
`interface(module, stub, said, parts)`.

**Binding a re-export.** `bind` makes the body it binds from the stub's own statements followed by
those its re-exports bring. For each name re-exported, the statements that define it in its
source are module-level `def`s (overloads and version branches included) and `class`es of that
name. When the source re-exports the name in turn, the binder follows it, each (module, name) once.
The statements are cloned, renamed for `as`, and appended, so a name the stub defines itself wins,
as the first definition does today. A star import brings the source's public names: its
`__all__`, else every module-level name not starting with `_`. The type variables of every part
are merged into the stub's (adr:0036). A class a re-exported signature names but nothing
re-exports is not a class of this stub, and so is a `PyObject`. A name whose source has no stub,
or follows past depth 4, is a `#   name: reason` line.

**Finding parts** (`lotml/src/stubs.rs`). `Stub` gains `parts`. `find` looks a part up through
`find` itself (typeshed for the standard library, else the `uv.lock` environment), and the
environment is made once for all parts. A stub's package is its module when its file is an
`__init__`, else its parent. `key()` and the lock's `stub` hash cover the text and every part, so
a changed submodule invalidates the cached interface and is reported as E0225 as a changed stub
is. `lotml bind --stub <file>` (`exec.rs`) reads a module under the stub's package from the file
beside it (`<dir>/<rest>.pyi`, `.py`, `<rest>/__init__.pyi`, `.py`), and finds any other module as
the compiler does.

**Coverage.** The label `generics` is the measurement before this spec; `reexports` is the one
after. `harness/results/bind-on-import.md` is measured again offline from the cached environment.

## Risks

- A stub's re-exports can name the same module through two paths; each module is read once and
  each (module, name) followed once, so the work is bounded by the parts read.
- A name re-exported from a module whose stub is a `.py` without annotations binds as that file
  binds: what is typed is typed, the rest a `PyObject` or left out.
