---
status: accepted
---

# 0030 · The binding coverage report reads stubs from pinned distributions

## Context

`specs/binding-coverage/` measures how much of a fixed corpus of Python modules `lotml bind`
types, so each spec of `plans/python-compatibility.md` that types more can show its gain as a
number. A number compared across months means nothing if the stubs it was taken from moved:
typeshed rewrites a module's stub between releases, and a package's own types change with its
version. `lotml bind` today reads typeshed from whatever mypy or jedi the user has installed
(`docs/stack.md`), which is the opposite of reproducible, and the harness has neither.

## Decision

The report reads every stub from a distribution pinned in `harness/uv.lock`, under the harness's
dependency group `stubs`: mypy for typeshed's `stdlib`, the `types-*` stub packages of typeshed's
third-party stubs, `pandas-stubs`, and the packages that carry their own types (numpy, urllib3,
packaging, idna, certifi, charset-normalizer). Each measurement records the version of every
distribution it read. The group is not a default one: CI does not install it, and the report is
run by hand when a spec changes the binder.

This decides where the report reads stubs from, and nothing about where the compiler does, which
stays open for task 2.3 of `plans/python-compatibility.md`.

Rejected:
- Fetching the stubs at measurement time: the network makes the corpus a moving target.
- Vendoring the stubs into the repository: tens of megabytes to keep current by hand, when the
  lock already pins them.
- Installing the packages themselves for the ones with separate stubs: the stubs are what `lotml
  bind` reads, and importing a package is never part of measuring it.

## Consequences

- A measurement is reproducible from the repository: `uv sync --group stubs` and the label.
- Upgrading a pinned distribution changes the numbers for a reason that is not the binder's, so
  the report marks a label measured on other versions than the latest.
- The harness's lock carries numpy and pandas-stubs, about 30 MB a developer downloads only when
  measuring.
