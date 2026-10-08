"""The binder `lotml bind` runs, where typeshed has no stub (plans/bind-sources.md 1.1): a
standard-library name is never left to the project's packages, and no source's name can add a
line to the interface."""

from lotml_harness import ROOT
from lotml_harness.experiments.binding_coverage import load_binder

binder = load_binder(ROOT / "compiler" / "crates" / "lotml-py" / "runtime" / "lotml_bind.py")


def test_a_standard_library_name_without_typeshed_is_refused_not_looked_up(monkeypatch, capsys):
    monkeypatch.setattr(binder, "typeshed", lambda module: None)
    assert binder.main(["subprocess"]) == 2
    assert "standard library" in capsys.readouterr().err
    assert binder.main(["not_a_stdlib_package"]) == 3, "lotml looks in the project's packages"


def test_a_source_name_cannot_add_a_line_to_the_interface(tmp_path):
    stub = tmp_path / "m.pyi"
    stub.write_text("def f(x: int) -> int: ...\n", encoding="utf-8")
    text = binder.interface("m", stub, "x\nfn evil() -> int ! PyError\r\x85y")
    assert "fn evil" not in text.splitlines()[1:], text
    assert text.splitlines()[0].startswith("# The Python module `m`")
    assert [line for line in text.splitlines() if line.startswith("fn ")] == [
        "fn f(x: int) -> int ! PyError"
    ]


def test_a_stub_too_deep_to_parse_is_an_error_not_a_traceback(tmp_path, capsys):
    stub = tmp_path / "deep.pyi"
    stub.write_text("x: " + "list[" * 5000 + "int" + "]" * 5000 + "\n", encoding="utf-8")
    assert binder.main(["deep", str(stub)]) == 2
    assert "cannot read" in capsys.readouterr().err
