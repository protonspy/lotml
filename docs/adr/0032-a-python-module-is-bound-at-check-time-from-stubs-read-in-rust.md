---
status: accepted
---

# 0032 · A Python module is bound at check time, from stubs read in Rust

## Context

adr:0029-foreign-modules-are-imported-by-origin-and-their-interfaces-generated decided that `import
py.<module>` needs no `lotml bind`: the compiler finds the module's stub and writes its interface
itself, running no Python, under closed roots. It left open where a stub is read from and how the
generated interface is kept stable.

Today the binder is `lotml_bind.py`, run by a CPython with Python's own `ast`, as
adr:0012-python-interop-through-checked-boundaries-and-interface-files has it; it finds typeshed in
an installed mypy or jedi. Two plans proposed pieces of the answer: `plans/bind-sources.md` 2.1, an
ADR on reading stubs without Python, not yet written; and `plans/python-via-uv.md` 2.1, typeshed
taken from a mypy wheel locked with hashes, not yet built. monty reads Python with
`ruff_python_parser` from crates.io and embeds a trimmed typeshed in its binary
(`research/prior-art/studies/monty.md`). An interface generated from whatever stub the machine has
binds differently on another machine or after an upgrade.

## Decision

When a program imports `py.<module>` and the project holds no `bindings/py.<module>.lotmli`,
`lotml check`, `run` and `test` bind the module themselves: they read its stub as data, in Rust,
and generate its interface; the module is never imported and no Python runs.

- **The parser** is `ruff_python_parser` (MIT, crates.io), pinned. The binder of `lotml_bind.py`
  moves into Rust and writes the same interface; `lotml bind` becomes the command that writes it out
  to `bindings/` for review or correction, and needs no Python either.
- **The roots, closed and in order** (adr:0029): the typeshed `stdlib` embedded in lotml, then the
  project's environment as adr:0033's resolution of it names (`<package>-stubs`, a `.pyi` beside
  the package, the package's own `.py` when it carries `py.typed`, in PEP 561's order). No `.pth`,
  `sitecustomize` or `pyvenv.cfg` widens them.
- **typeshed is embedded at a pinned commit**: its `stdlib` stubs, with typeshed's licence, compressed
  into the binary at build time. A pin moves by a pull request that changes the commit, and the
  binding coverage report is run on it.
- **The lock.** `lotml.lock` at the project root records, for each `py.` module bound, the source
  read (the embedded typeshed's commit, or the distribution and version), the hash of the stub, and
  the hash of the interface generated. A check that binds a module the lock names from a stub whose
  hash differs reports it and binds from the stub found; `lotml bind --lock` rewrites the lock. The
  generated interfaces are cached per user, keyed by the stub's hash and lotml's version, and
  checked when read.
- **`plans/bind-sources.md` 2.1** is this record; its 1.1, a package's own `.pyi` and annotated
  `.py`, is the order above. **`plans/python-via-uv.md` 2.1 no longer stands**: the embedded
  typeshed replaces the mypy wheel, and `bind` needs neither Python nor a download.

Rejected:
- Importing the module and reading its signatures with `inspect`: it runs the module's code on
  every check, which adr:0029 rules out for untrusted projects.
- Keeping Python's `ast` as the parser: `lotml check` would need a CPython, and a check of an
  untrusted project would run one.
- A mypy wheel downloaded on first use: the check would need the network, and a download per user.
- typeshed from whatever mypy or jedi is installed: a binding that changes with the machine.

## Consequences

- `lotml check` binds Python with no Python installed, and the same lock gives the same interfaces
  on every machine.
- The binary grows by the compressed `stdlib` stubs, about 2 MB.
- lotml carries a Python parser, a dependency to keep current with the language's grammar, and
  becomes responsible for reading `.pyi` files correctly.
- `lotml_bind.py` and its tests move to Rust; the binding coverage report measures the Rust binder
  through `lotml bind`.
- A project's interfaces depend on its environment's versions, recorded and checked through the
  lock, which `lotml check` reports against rather than silently follows.
