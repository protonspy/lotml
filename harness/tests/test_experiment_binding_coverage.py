"""The binding coverage report (specs/binding-coverage): a fixed corpus, each module's stub found as
PEP 561 orders them and never imported, its public names counted and those `lotml bind` binds,
and each measurement kept under a label beside the others."""

import ast
import json
from pathlib import Path

from lotml_harness.experiments import binding_coverage as bc

BINDER = bc.load_binder(bc.BINDER)


def write(path: Path, text: str = "") -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")
    return path


def names(source: str) -> set[str]:
    return bc.public_names(ast.parse(source))


def test_the_corpus_names_the_standard_library_and_pypi_each_with_its_stub_distribution():
    modules = bc.corpus(bc.CORPUS.read_text(encoding="utf-8"))
    groups = {m.group for m in modules}
    assert groups == {"stdlib", "pypi"}
    assert all(m.distribution for m in modules)
    assert len({m.name for m in modules}) == len(modules), "each module once"
    assert {"random", "math", "requests", "numpy", "pandas"} <= {m.name for m in modules}


def test_every_distribution_of_the_corpus_is_pinned_in_the_stubs_group():
    project = (bc.ROOT / "harness" / "pyproject.toml").read_text(encoding="utf-8")
    group = project[project.index("stubs = [") :].split("]")[0].lower()
    for m in bc.corpus(bc.CORPUS.read_text(encoding="utf-8")):
        assert f'"{m.distribution.lower()}==' in group, m.distribution


def test_public_names_are_the_stub_s_all_when_it_has_one():
    assert names("__all__ = ['a', 'b']\n__all__ += ['c']\ndef a(): ...\ndef d(): ...\n") == {
        "a",
        "b",
        "c",
    }


def test_public_names_without_all_are_what_the_stub_defines_or_re_exports():
    source = """import sys
from os import path as path, sep
import json as json
import re
def f() -> int: ...
class C: ...
x: int
y = 1
_hidden = 2
if sys.version_info >= (3, 12):
    def g() -> int: ...
"""
    assert names(source) == {"path", "json", "f", "C", "x", "y", "g"}


def test_a_stub_distribution_comes_before_the_package_and_a_py_file_counts_only_when_typed(
    tmp_path,
):
    purelib = tmp_path / "site"
    module = bc.Module("pkg.sub", "pypi", "types-pkg")
    write(purelib / "pkg" / "sub.py", "def f() -> int: ...\n")
    assert bc.stub_of(module, None, purelib) is None, "untyped source is no stub"
    write(purelib / "pkg" / "py.typed")
    assert bc.stub_of(module, None, purelib) == purelib / "pkg" / "sub.py"
    write(purelib / "pkg" / "sub.pyi")
    assert bc.stub_of(module, None, purelib) == purelib / "pkg" / "sub.pyi"
    write(purelib / "pkg-stubs" / "sub" / "__init__.pyi")
    assert bc.stub_of(module, None, purelib) == purelib / "pkg-stubs" / "sub" / "__init__.pyi"


def test_a_standard_library_module_is_read_from_typeshed(tmp_path):
    typeshed = tmp_path / "stdlib"
    write(typeshed / "os" / "__init__.pyi")
    write(typeshed / "os" / "path.pyi")
    assert (
        bc.stub_of(bc.Module("os", "stdlib", "mypy"), typeshed, tmp_path)
        == typeshed / "os" / "__init__.pyi"
    )
    assert (
        bc.stub_of(bc.Module("os.path", "stdlib", "mypy"), typeshed, tmp_path)
        == typeshed / "os" / "path.pyi"
    )
    assert bc.stub_of(bc.Module("os", "stdlib", "mypy"), None, tmp_path) is None


def test_a_module_counts_the_public_names_lotml_bind_binds_and_one_without_a_stub_counts_zero(
    tmp_path,
):
    purelib = tmp_path / "site"
    write(
        purelib / "rand-stubs" / "__init__.pyi",
        "class R:\n    def roll(self, n: int) -> int: ...\n_inst: R\nroll = _inst.roll\n"
        "def pick(xs: object) -> int: ...\ndef seed(n: int) -> None: ...\n",
    )
    measured = bc.measure(bc.Module("rand", "pypi", "types-rand"), BINDER, None, purelib)
    assert (measured.stub, measured.public, measured.bound) == (
        "rand-stubs/__init__.pyi",
        4,
        ["roll", "seed"],
    )
    assert measured.share == 0.5
    missing = bc.measure(bc.Module("absent", "pypi", "absent"), BINDER, None, purelib)
    assert (missing.stub, missing.public, missing.share) == (None, 0, 0.0)


def record(label: str, taken: str, bound: int, versions: dict | None = None) -> bc.Record:
    modules = [
        bc.Module("random", "stdlib", "mypy", "random.pyi", 4, ["a", "b", "c", "d"][:bound]),
        bc.Module("numpy", "pypi", "numpy", "numpy/__init__.pyi", 10, []),
    ]
    return bc.Record(label, modules, versions or {"mypy": "1", "numpy": "2"}, "digest", taken)


def test_a_record_reads_back_as_it_was_written():
    r = record("aliases", "2026-10-08T00:00:00+00:00", 3)
    again = bc.load(bc.dump(r))
    assert again == r
    assert json.loads(bc.dump(r))["versions"] == {"mypy": "1", "numpy": "2"}


def test_the_report_shows_every_label_and_marks_one_measured_on_other_versions():
    before = record("functions", "2026-10-08T00:00:00+00:00", 1, {"mypy": "0", "numpy": "2"})
    after = record("aliases", "2026-10-08T01:00:00+00:00", 3)
    report = bc.markdown([after, before])
    assert report.index("| functions |") < report.index("| aliases |"), "in the order taken"
    assert "| functions | 25.0% | 0.0% | 7.1% | another corpus or other versions |" in report
    assert "| aliases | 75.0% | 0.0% | 21.4% | the latest corpus and versions |" in report
    assert "| random | mypy | 4 | 3 | 75.0% |" in report
