---
autonomy: auto
ci: wait
branch: feat/python-compat-group-1
delivery: in-progress
---

# Binding coverage — requirements

## Purpose

How much of Python a LotML program can call typed, measured on a fixed corpus so a change to the
binder shows as a number (plans/python-compatibility.md 1.2). The report is the before and the
after of each spec that types more of a module's API: classes, overloads, generics.

## R1 · The corpus

- **R1.1** The harness shall name in one file the modules measured: the standard-library modules a program most often imports, and the most downloaded PyPI packages, each with the distribution its stub comes from.
- **R1.2** The harness shall read each module's stub from a distribution pinned in its lock, and run no network access and no module of the corpus while it measures.
- **R1.3** If no stub of a module of the corpus is found, then the harness shall report that module as having no stub, with a share of 0, rather than leave it out.

## R2 · What is counted

- **R2.1** The harness shall count as a module's public names the names its stub's `__all__` lists, or, without one, the names the stub defines or re-exports at module level that do not start with `_`.
- **R2.2** The harness shall count as bound typed each public name `lotml bind` writes as a function of the module's interface from that stub.

## R3 · The report

- **R3.1** The harness shall keep each measurement under a label, and report every label recorded beside the others: per module its public names, those bound typed and the share, and the share of the standard library, of PyPI and of the whole corpus.
- **R3.2** When the corpus or the pinned distributions change, the harness shall say so in the report beside the labels measured on the old ones.

## Out of scope

- Finding stubs for `lotml bind` or `lotml check`: the report reads the pinned ones, and how the
  compiler finds a stub is `plans/bind-sources.md` and task 2.3 of the plan.
- Whether a bound signature is right: the report counts what is bound, and the boundary of
  adr:0012 checks every value that crosses.
