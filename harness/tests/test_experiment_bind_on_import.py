"""Binding on import over the corpus (specs/bind-on-import R4.1): each module imported as
`py.<module>` and run with no `lotml bind`, the PyPI ones in a project that locks them, every
failure reported."""

from lotml_harness.execute import child_environment
from lotml_harness.experiments import bind_on_import as boi
from lotml_harness.experiments.binding_coverage import CORPUS, Module, corpus


def test_a_stub_distribution_is_installed_beside_the_distribution_it_types():
    assert boi.runtime("types-requests") == "requests"
    assert boi.runtime("types-PyYAML") == "PyYAML"
    assert boi.runtime("pandas-stubs") == "pandas"
    assert boi.runtime("numpy") == "numpy"


def test_the_scratch_project_locks_each_pypi_module_with_its_stub_pinned_as_coverage_reads_it():
    pinned = boi.pins(boi.PYPROJECT.read_text(encoding="utf-8"))
    assert pinned["numpy"].startswith("numpy==")
    manifest = boi.pyproject(corpus(CORPUS.read_text(encoding="utf-8")), pinned)
    for wanted in [pinned["types-requests"], '"requests"', pinned["pandas-stubs"], '"pandas"']:
        assert wanted in manifest, wanted
    assert manifest.count(pinned["numpy"]) == 1, "a distribution that types itself is listed once"
    assert "textwrap" not in manifest, "the standard library is not installed"


def test_a_standard_library_module_runs_with_no_bind_and_one_no_stub_binds_fails_saying_why(
    tmp_path,
):
    (tmp_path / ".git").touch()
    env = child_environment() | {"LOTML_CACHE_DIR": str(tmp_path / "cache")}
    ran = boi.run(boi.COMPILER, tmp_path, Module("textwrap", "stdlib", "mypy"), env)
    assert ran.passed, ran.said
    assert not (tmp_path / "bindings").exists()
    failed = boi.run(boi.COMPILER, tmp_path, Module("nowhere", "pypi", "nowhere"), env)
    assert not failed.passed
    assert "E0216" in failed.said, failed.said


def test_the_report_counts_what_ran_and_names_each_failure():
    report = boi.markdown(
        [
            boi.Outcome("textwrap", "stdlib", True),
            boi.Outcome("pandas", "pypi", False, "error[E0216] | no stub"),
        ]
    )
    assert "1 of 2 modules ran." in report
    assert "| textwrap | stdlib | ran |" in report
    assert "| pandas | pypi | failed: error[E0216] \\| no stub |" in report
