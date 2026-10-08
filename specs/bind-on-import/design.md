# Bind on import — design

## What changes

Serves R1.1, R1.2, R1.3, R1.4, R1.5, R1.6, R2.1, R2.2, R2.3, R3.1, R4.1, R4.2.

**One way to a stub.** `lotml bind`'s resolution in `compiler/crates/lotml/src/exec.rs` moves into
a function the compiler shares: the embedded typeshed (`lotml_bind::typeshed::find`) for the
standard library, else the packages of the lock's environment (`lotml_py::sources::find` over its
`site-packages`), as adr:0032 and adr:0033 order the roots. It answers the stub's text, where it
came from and the stub's SHA-256, or why there is none. `lotml_py::resolve::environment` —
`VIRTUAL_ENV`, then `.venv` — is no longer a root for any command (R1.2), so `lotml bind` and the
checker always generate the same interface.

**Finding the lock's environment without Python.** `lotml run` names an environment by hashing the
lock, the manifest and the interpreter, which it learns by running that interpreter
(`dependencies::key`, `exec.rs`'s `executable_of`). A check runs no Python, so it looks under
`python-environments/` for a completed environment whose copied `project/uv.lock` and
`project/pyproject.toml` (written by `dependencies::build`) equal the project's, newest first. None
found: modules outside the standard library are reported as unbound, with the command that makes
the environment (adr:0033). `run` and `test` make the environment before they collect interfaces
(R1.3); today they collect them first.

**Where interfaces are collected.** Every command reads them through `files::interfaces_for` —
`check`, `run` and `test` directly, the language server and the MCP server through
`InterfaceCache`. That function gains the file's imports: for each `py.<module>` with no
`bindings/py.<module>.lotmli`, it adds the generated interface (R1.1); for a bindings file that
stands where one would be generated, it records the shadowing for the warning (R1.6). The checker
(`lotml-check`, which depends on nothing outside the compiler) is given what it needs as data: the
interfaces, the shadowed names, the reasons a module was not bound (R1.4), and which bare imports
the embedded typeshed covers (R1.5). It reports them; it never resolves a stub itself.
`InterfaceCache` keys its entries by directory and by the imports read, so an edit adding an
import binds that module once.

**The checker's reports.** `E0216` for `py.<module>` keeps its code; its note becomes the reason the
module was not bound, ending with "`lotml bind <module>` tells why" (R1.4). A bare `import <name>`
that is no LotML module and that typeshed covers gets the machine-applicable fix to `py.<name>`
(R1.5). A shadowing bindings file is a warning at the import (R1.6).

**The binding coverage report** (`harness/lotml_harness/experiments/binding_coverage.py`) binds
standard-library modules through `lotml bind <module>` without `--stub`, so it measures the typeshed
`import py.<module>` binds, and records `lotml_bind::typeshed::COMMIT` (R4.2). The corpus run
(R4.1) imports each module under `lotml run` with no `bindings/`; the PyPI modules run in one
scratch project whose `uv.lock` pins them.

## Data

`lotml.lock`, at the project root, TOML, written only by `lotml bind --lock`:

```toml
version = 1

[[module]]
name = "py.random"
source = "typeshed 1a2b3c4d5e6f"   # or "<distribution> <version>" from its dist-info
stub = "sha256:…"
interface = "sha256:…"
```

Sorted by name, one entry per `py.` module a program of the project imports. It is read as data: a
name that is not a dotted ASCII identifier is an error, and the lock is never a root (adr:0032).

The cache of generated interfaces is `interfaces/` under `lotml_llvm::cache::user_root()`, made
with `private_directory`, one file per `<stub sha256>-<lotml version>.lotmli`, written to a
temporary name and renamed into place. An entry for a module the lock names is used only when its
SHA-256 is the lock's `interface` (R3.1); otherwise it is generated again.

## Boundaries and contracts

- `lotml bind <module>` stops reading `VIRTUAL_ENV` and `.venv` (R1.2). A project that bound a
  PyPI package from its `.venv` makes its `uv.lock` environment once with `lotml run`, or passes
  `--stub`.
- `lotml check --locked` is new and exits non-zero on a missing lock, a differing stub, or an
  unlisted import (R2.3); `lotml bind --lock` is new (R2.1).

## Risks

- The language server binds on the first open of a file importing a large stub. The size, depth
  and time limits the binder already enforces bound it, and the cache makes it once per stub.
- An environment made by an older lotml with the same lock is found and read. Its stubs are those
  of the lock's versions, which is what R1.2 asks; its interpreter version may differ, which no
  stub depends on.
