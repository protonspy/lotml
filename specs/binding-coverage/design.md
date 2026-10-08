# Binding coverage — design

## The corpus

Serves R1.1, R1.2.

`harness/binding-corpus.json` names the modules: 25 of the standard library a program most often
imports, and the 11 most downloaded PyPI packages whose stubs a pinned distribution carries. Each
entry names the distribution its stub comes from, `mypy` for the standard library, whose wheel
carries typeshed's `stdlib`. The distributions are the harness's dependency group `stubs`, pinned
in `harness/uv.lock` (adr:0030-the-binding-coverage-report-reads-stubs-from-pinned-distributions),
so a measurement is reproducible from the lock alone. The group is not a default one: the report
is run by hand, and CI tests the counting on stubs of its own.

## Finding a stub

Serves R1.2, R1.3.

A standard-library module is read from mypy's `typeshed/stdlib`, found with
`importlib.util.find_spec("mypy")`, which locates the package without importing it. A PyPI module
is looked up in the environment's `purelib` in the order PEP 561 gives: `<top>-stubs`, then a
`.pyi` beside the package, then the package's `.py` when it carries `py.typed`. No module of the
corpus is imported, and nothing is fetched. A module with no stub is reported with a share of 0.

## Counting

Serves R2.1, R2.2.

- Public names: the string literals of a module-level `__all__`, or else every module-level
  `def`, `class` and assigned or annotated name, and every `import … as name` and `from … import
  name as name` re-export, PEP 484's form, that does not start with `_`. Version checks are walked
  as `lotml bind` walks them.
- Bound typed: the names `lotml_bind.interface` writes as `fn`, the binder loaded from
  `compiler/crates/lotml-py/runtime/lotml_bind.py`, or from the file `--binder` names, so a binder
  of another revision can be measured against the same stubs.

## The report

Serves R3.1, R3.2.

`python -m lotml_harness.experiments.binding_coverage <label>` keeps the measurement in
`results/binding-coverage/<label>.json`, with the corpus and the version of each distribution read,
and rewrites `results/binding-coverage.md` from every label recorded, in the order they were
taken: one table of the shares by label, then each label's modules. A label measured on another
corpus or other versions than the latest is marked so beside its share.
