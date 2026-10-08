---
status: accepted
---

# 0029 · Foreign modules are imported by origin, and the compiler generates their interfaces

## Context

A LotML program reaches foreign code through interfaces of bodyless signatures,
`bindings/<module>.lotmli`. adr:0012-python-interop-through-checked-boundaries-and-interface-files
has `lotml bind` write a Python module's interface from its stub, as a step the programmer runs
before the import works, and resolves a bare `import random` to that file.
adr:0013-c-libraries-through-interfaces-named-c names a C library's interface `c.<library>` and
has it written by hand, rejecting header reading because macros and platform `#if`s make a header
a program rather than a list of types.

Trying it on 2026-10-08 showed the cost. `import random` failed with E0216 until `lotml bind` ran;
`lotml bind random` then needed a stub found by hand, since no mypy or jedi was installed; and the
typeshed stub bound 0 functions, because its names are `randint = _inst.randint` aliases. The
owner's aim is a language that reaches every Python module and C library with an import and
nothing else. Two facts make that reachable. Other compilers already read foreign declarations
on import: Zig's `@cImport` and Swift's Clang importer read C headers through Clang, which
preprocesses the macros and `#if`s for the target platform, and TypeScript finds a package's
types without a manual step. And `lotml build` already requires `clang`, so the header reader is
installed wherever native builds run. A bare import also hides the module's origin: the reader
of `import random` cannot tell a LotML module from a Python one, and the rules a call follows
differ, `T ! PyError` for Python and `T` for C.

## Decision

A foreign module is imported by origin, and the compiler writes its interface itself. The owner
decided this on 2026-10-08. This amends adr:0012 on the required `lotml bind` step and the bare
import, and adr:0013 on hand-written interfaces and the rejection of header reading; the rest of
both records stands.

- `import py.<module>` and `from py.<module> import <name>` reach a Python module, for example
  `import py.random`. The compiler finds the module's stub and generates the interface. Every
  call still returns `T ! PyError`, and every value still crosses the checked boundary of adr:0012.
- `import c.<library>` and `from c.<library> import <name>` reach a C library, for example
  `import c.m` for the C math library. The compiler reads the library's header through Clang and generates the
  interface. The types that cross stay those of adr:0013 until a later decision widens them, and
  a declaration outside them is listed as not bound, with the reason.
- A bare `import <name>` names a LotML module only. When no LotML module has that name and a
  Python stub for it is found, the error suggests `py.<name>`; finding the stub runs no Python.
- A `bindings/<origin>.<module>.lotmli` checked into the project takes precedence over the
  generated interface. It is how a binding gets corrected by hand. `lotml bind` stays as the
  command that writes the generated interface out for review.

`lotml check` of untrusted code stays a supported use, such as CI on a fork's pull request, so
generating an interface holds to these rules:
- **No Python runs.** A stub is found by reading directories under closed roots: the typeshed
  lotml embeds, then the installed packages' `.pyi` files and `py.typed` packages. No interpreter
  is spawned, and no `.pth`, `sitecustomize` or `pyvenv.cfg` widens the roots. A test checks a
  `py.` import with no Python on `PATH`.
- **Clang runs with arguments lotml fixes.** It runs with `-fsyntax-only` and an error limit, no
  `-I`, `-D`, `-include` or plugin taken from the project, and `CPATH` and the `*_INCLUDE_PATH`
  variables cleared. It runs under a timeout, and the header and AST sizes are capped.
- **The header search is a closed, ordered list.** The project's `include/` inside its root comes
  first, then the system directories. Every file the header includes must resolve, symlinks
  followed, inside one of them. Two candidates for one name are an error, not a choice.
- **A name reaches a file only by lookup.** Module and library names are ASCII identifier segments
  looked up in those roots. No separator, `..`, absolute path or device name ever reaches a path
  join, and names that differ only by case are an error.
- **An override is trusted code and is checked.** `check` reports each `bindings/` file that
  shadows a generated interface, and holds it to the same rules: a `py.` signature keeps
  `! PyError`, and a `c.` one uses only adr:0013's types. `bindings/` stays confined to the
  repository, as it is today.
- **The cache is per user**, keyed by the content hash of the stub or header, the Clang version and
  the target triple, and checked when read. The lock records the target triple too.

What stays open, for records of their own: where a stub is read from and how the generated
interface is kept stable (task 2.3 of `plans/python-compatibility.md`), and how a project's
dependencies are installed (its task 2.5).

Rejected:
- Keeping the bare `import random` for Python. It hides which rules a call follows, and a
  LotML module of the same name would shadow it.
- `import PythonModules<random>`. Angle brackets read as generics, and `py.` follows the `c.`
  prefix adr:0013 already uses.
- Keeping `lotml bind` as a required step (adr:0012) and hand-written C interfaces (adr:0013).
  Each is the friction this record removes.

## Consequences

- Every program, test and page that imports Python with a bare name moves to `py.`:
  `compiler/crates/lotml/tests/interop.rs`, the agent guide and `reference/lotml.md`. `c.m` keeps
  its name, and only stops needing a hand-written file.
- An interface now depends on the environment: the installed package's version and the
  platform's headers. The same program can bind differently on Windows and Linux, or after an
  upgrade, which is why the generated interface needs a lock.
- `lotml check` now runs a C preprocessor and parser over input a project controls. The rules
  above bound what that can read and how long it can run, but Clang's own parser becomes part of
  what a check exposes.
- Clang becomes a requirement of `lotml check` for a file that imports `c.`, not only of
  `lotml build`.
- Function-like macros, unions, varargs, pointers and callbacks in a header are not bound until
  a decision gives them an ownership rule. Like adr:0013's, these limits are recorded as ceilings.
