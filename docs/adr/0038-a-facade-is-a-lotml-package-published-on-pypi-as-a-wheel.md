---
status: proposed
---

# 0038 · A facade is a LotML package, published on PyPI as a wheel

## Context

A program reaches a Python library through `import py.<module>`, whose interface the compiler
generates from the library's stub (adr:0029-foreign-modules-are-imported-by-origin-and-their-interfaces-generated).
That reaches every module, but the interface has the library's Python shape:
- every call returns `T ! PyError`;
- a class crosses as a handle reached only through its declared members
  (adr:0034-a-python-class-crosses-as-a-nominal-handle-with-its-declared-members);
- what the stub cannot type crosses as `PyObject`
  (adr:0031-a-python-name-no-stub-types-crosses-as-an-opaque-python-value).

Some libraries are shaped in ways LotML does not express, and its syntax is frozen
(adr:0010-variant-b-and-indented-blocks-settled-by-the-phase-0-gate):
- a connection or session used through `with`, as in redis, pymongo and sqlalchemy;
- a framework that calls the program back, as FastAPI calls a route handler;
- an error reported as one of a dozen exception classes.

Binding more of Python's type system narrows this gap one feature at a time
(plans/python-compatibility.md). It does not give a library an API designed for LotML. A
`bindings/py.<module>.lotmli` file can rename and narrow an interface, but it holds no code. So it
cannot keep a connection, map exceptions to errors, or hide a context manager.

The owner wants the community to bring libraries to LotML, each through a package written for it.

These already hold:
- lotml ships uv (adr:0026-lotml-ships-uv-and-runs-python-3-14-by-default);
- a project's dependencies come from its `uv.lock`, installed as wheels from PyPI with every hash
  checked and no build code run
  (adr:0033-lotml-run-installs-a-project-s-python-dependencies-from-its-uv-lock);
- the compiler reads stubs from that environment without running Python
  (adr:0032-a-python-module-is-bound-at-check-time-from-stubs-read-in-rust).

LotML has no package format and no registry of its own. The compiler is distributed as a release
archive, not on PyPI.

## Decision

Proposed, for the owner to rule on. This amends adr:0029 on where a LotML module is found; the
rest of that record stands.

**A LotML package is a wheel.** It is published on PyPI and declared and locked like any Python
dependency (adr:0033). It carries LotML source under a directory its manifest names, and the
range of lotml versions it was written for. The compiler finds packages in the project's
environment by reading files, as it finds stubs: no Python runs, so `lotml check` of untrusted
code stays safe (adr:0029). A package whose lotml range excludes the running compiler is an error
that names both.

**A package is imported bare.** `import redis` names a LotML module, as adr:0029 has it. Today a
bare import reaches `math` alone: a LotML file importing another is decided by a record of its own
(plans/facades.md 1.1), and a package's modules are a second root beside the project's.
uv installs and locks the packages; the compiler finds and links the modules in them. Two candidates for one name, from
the project and a package or from two packages, are an error naming both, never a choice.
`import py.redis` still reaches the Python library directly, so a program can use what a package
leaves out.

**A facade is a package that gives a foreign library a LotML-shaped API.**
- It is LotML source over `py.` imports. A facade over `c.` waits for a rule on a package's own
  headers, since adr:0029 searches only the project's `include/` and the system. It maps the library's exceptions to its own
  error values (adr:0002-errors-as-values) and its handles to records where a value is copied.
- Where LotML cannot express a part, such as a `with` block or a callback, the facade ships a
  Python module of its own, with a stub. It imports that module as `py.<module>` like any other.
  Every value from it is still checked at the boundary
  (adr:0012-python-interop-through-checked-boundaries-and-interface-files), so a wrong value from
  the facade's Python becomes an error, never a LotML value of the wrong type.
- A facade's distribution is named `lotml-<library>`, and its module takes the library's name.

**The community publishes facades.** The project keeps an index of reviewed facades. Each entry
records:
- the distribution, its version and its wheel's SHA-256;
- the lotml range the facade declares;
- the library version it was tested against.

A facade enters the index by a pull request. The project's CI then runs the facade's own tests
against the real library, without secrets. The compiler does not consult the index, and any
package installs. The index is what the guide recommends and what reviewers trust. The project
maintains reference facades in its own repository, as examples and as the conformance fixtures
for the format. Every other facade lives in its author's repository.

**Ruled out:**
- **Extras on the compiler, `lotml[redis]`.** lotml is not on PyPI, and extras would tie every
  facade's release to a compiler release.
- **A registry of LotML's own**, like crates.io. That is a service to operate and a resolver to
  write, when PyPI and uv are already in the toolchain and already hash-checked.
- **Every facade in this repository.** That gives one maintainer every library, and the compiler's
  release cadence to each of them.
- **Published interface files without code.** They cannot reshape an API, which is the whole point
  of a facade.
- **Git or path packages.** adr:0033 refuses those sources, and they would bypass the lock's
  hashes.

## Consequences

- A library LotML cannot use well through `py.` can get an API designed for LotML without a
  language change. The language work in plans/python-compatibility.md still raises the floor
  under every library a facade does not cover.
- A facade over `py.` stays on the Python target: `build` refuses its import (E0401) until a
  native implementation of the same API is decided. A pure LotML package builds natively too,
  through the linking of several files the module record decides for the LLVM target.
- LotML's package registry is PyPI, with PyPI's namespace and policies.
  - Anyone can publish `lotml-<anything>`, so squatting and typosquatting are possible. The lock's
    hashes pin what was chosen. The index says what was reviewed.
  - Nothing stops an unreviewed package from being installed.
- At `lotml run`, a facade's Python module runs with a dependency's trust. The boundary checks its
  values, not its effects. A facade can do anything its Python can.
- The language is not stable, so a facade breaks across lotml releases. The declared lotml range
  turns that into an error at import, rather than a failure inside the facade.
- Two facades for one library cannot be installed in one project, because they claim one module
  name.
- A facade is a second way to reach a library, beside `py.`. The guide has to say when to use
  which, or a model writing LotML will pick either at random.
