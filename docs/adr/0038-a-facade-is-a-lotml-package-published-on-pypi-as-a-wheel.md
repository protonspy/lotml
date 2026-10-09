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
The owner also wants only listed packages to install. Anyone can publish a `lotml-` name on PyPI,
and the models that write LotML invent package names.

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

Proposed, for the owner to rule on. This amends adr:0029 on where a LotML module is found, and
adr:0033 on which locked distributions a run installs; the rest of both records stands.

**A LotML package is a wheel.** Each version is one `py3-none-any` wheel on PyPI, with no sdist,
declared and locked like any Python dependency (adr:0033). A manifest inside the wheel names:
- the modules the package provides;
- the directory that holds their LotML source;
- the range of lotml versions the package was written for.

A package whose range excludes the running compiler is an error that names both.

**Only listed packages are installed.** This repository keeps `index/packages.toml`, the allowlist
of LotML packages. Each entry records:
- the distribution, its name normalized as PEP 503 has it;
- its version and its wheel's SHA-256;
- the modules it provides;
- its lotml range, and for a facade the library version its tests ran against.

lotml fetches the index from this repository over HTTPS and caches it. Offline it uses the cache.
With neither, it installs no LotML package: it fails closed.
- **Before installing.** `lotml run` and `lotml test` read `uv.lock`. A `lotml-` distribution, or
  one the index names, must be listed at that version, and the lock's hash must equal the
  index's. Otherwise nothing is installed, and the error names the distribution.
- **After installing.** Before any Python starts, the environment is scanned for manifests. A
  distribution that carries one and is not listed, or not listed at that version, stops the run.
- **Revocation.** An entry removed from the index revokes it: the next run with a fresh index
  refuses an environment that holds it.
- **Other dependencies.** A Python dependency without a manifest is not restricted by the index.
  The lock's hashes govern it, as adr:0033 has it, or `py.` would reach only what the index lists.

**A project may list private packages.** A file at the project root lists extra entries with the
same fields, each with its hash. lotml warns on each one as outside the index. An extra entry
cannot replace an index entry of the same name.

**A package is imported bare.** `import redis` names a LotML module, as adr:0029 has it. Today a
bare import reaches `math` alone. A LotML file importing another is decided by a record of its own
(plans/facades.md 1.1), and a package's modules are a second root beside the project's. uv
installs and locks the packages. The compiler finds and links the modules in them.
- Only a package the project declares directly in `pyproject.toml` adds a module root. A
  transitive one adds none.
- A package provides only the modules its entry lists. A module lotml embeds, such as `math`,
  cannot be claimed.
- Two candidates for one name are an error naming both, never a choice: the project and a
  package, or two packages. A name a package provides is never suggested as `py.<name>`.
- A package adds no `bindings/` file. adr:0029's override stays the project's own.
- `import py.redis` still reaches the Python library directly, so a program can use what a package
  leaves out.

**`lotml check` stays safe on untrusted code.** It reads packages only from the environment in
lotml's cache that is keyed by the lock (adr:0033), never from a `.venv` in the project tree. It
runs no Python. A manifest's paths and a package's source are confined and capped, as adr:0029
confines and caps stubs.

**A facade is a package that gives a foreign library a LotML-shaped API.**
- It is LotML source over `py.` imports. A facade over `c.` waits for a rule on a package's own
  headers, since adr:0029 searches only the project's `include/` and the system.
- It maps the library's exceptions to its own error values (adr:0002-errors-as-values), and its
  handles to records where a value is copied.
- Where LotML cannot express a part, such as a `with` block or a callback, the facade ships a
  Python module of its own, with a stub. It imports that module as `py.<module>` like any other.
  Every value from it is still checked at the boundary
  (adr:0012-python-interop-through-checked-boundaries-and-interface-files), so a wrong value from
  the facade's Python becomes an error, never a LotML value of the wrong type.
- A facade's distribution is named `lotml-<library>`, and its module takes the library's name.

**The community publishes packages.** A package enters the index by a pull request.
- **What review covers.** The source at the tagged commit, and the wheel's provenance attestation
  (PEP 740) naming that repository and its release workflow.
- **What CI checks.** It verifies the attestation and the hash, then runs the package's tests in
  an environment locked by hash. It runs on `pull_request` with read-only permissions, no secrets
  and no OIDC token.
- **What an entry vouches for.** Exactly the listed version and hash. A new release needs a new
  entry.
- **Where packages live.** The project maintains reference facades in its own repository, as
  examples and as the conformance fixtures for the format. Every other package lives in its
  author's repository.

**Ruled out:**
- **An advisory index the compiler does not consult.** `uv add lotml-<name>` would then install
  whatever was published first under that name.
- **An index embedded in each lotml release.** A new package would need a compiler release, and a
  revoked one would stay installable until the next.
- **Restricting every Python dependency.** `py.` would reach only what the index lists.
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
  native implementation of the same API is decided. A pure LotML package, one that reaches no
  `py.` or `c.` module even through another package, builds natively too. It does so through the
  linking of several files the module record decides for the LLVM target.
- An unlisted LotML package does not install, so typosquatting a `lotml-` name reaches no project.
  PyPI's namespace is still open. The project registers `lotml` and the names of its reference
  facades before it announces them, and the guide names exact distributions.
- Whoever can merge to `index/packages.toml` admits a package for every user. That repository's
  branch protection is now part of every project's supply chain.
- A first run needs the network to fetch the index. A revocation reaches a user at their next
  online run, not before.
- Review is the bottleneck for the community. The project's own list of private packages is the
  escape, and it is explicit and warned.
- A listed package is still arbitrary code at `lotml run` and `lotml test`:
  - its LotML can import `py.os`;
  - its Python module runs, and its `.pth` files run when the interpreter starts.

  The boundary checks values, not effects. Review is what stands between a package and the user's
  machine.
- The language is not stable, so a package breaks across lotml releases. The declared lotml range
  turns that into an error at import, rather than a failure inside the package.
- Two packages that provide one module cannot be installed in one project.
- A facade is a second way to reach a library, beside `py.`. The guide has to say when to use
  which, or a model writing LotML will pick either at random.
