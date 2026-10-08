"""The conformance matrix: the language rules the LLVM target's parity programs prove, from a
manifest checked against the tests themselves."""

import json

from lotml_harness.experiments import conformance

MANIFEST = {
    "rules": {"a": "rule a", "b": "rule b", "c": "rule c"},
    "programs": {
        "one": {"proves": ["a"], "leaves_out": {"b": "proved by two"}},
        "two": {"proves": ["a", "b"], "leaves_out": {}},
    },
}


def test_the_committed_manifest_describes_every_parity_program_and_only_them():
    manifest = json.loads(conformance.MANIFEST.read_text(encoding="utf-8"))
    found = conformance.programs_in()
    assert len(found) > 60
    assert conformance.problems(manifest, found) == []


def test_the_tests_programs_are_read_from_every_kind_of_call(tmp_path):
    calls = 'parity("p1", "…");\npanics(\n    "p2",\n    "…",\n);\nfrees_everything("p3", S);\n'
    (tmp_path / "x.rs").write_text(calls, encoding="utf-8")
    assert conformance.programs_in(tmp_path) == {"p1": "x", "p2": "x", "p3": "x"}


def test_a_manifest_out_of_step_with_the_tests_is_refused():
    found = {"one": "f", "three": "f"}
    wrong = conformance.problems(MANIFEST, found)
    assert "`three` runs in f.rs and the manifest does not name it" in wrong
    assert "`two` is in the manifest and in no test" in wrong
    broken = {"rules": {"a": "x"}, "programs": {"one": {"proves": [], "leaves_out": {"z": " "}}}}
    wrong = conformance.problems(broken, {"one": "f"})
    assert "`one` proves no rule" in wrong
    assert "`one` names the rule `z`, which the manifest does not declare" in wrong
    assert "`one` leaves out `z` without a reason" in wrong


def test_the_matrix_names_each_rule_s_programs_its_exceptions_and_its_gaps():
    report = conformance.matrix(MANIFEST, {"one": "f", "two": "f"})
    assert "| `a`: rule a | `one`, `two` |  |" in report
    assert "| `b`: rule b | `two` | `one`: proved by two |" in report
    assert "| `c`: rule c | **none** |  |" in report
    assert "2 programs prove 2 of 3 rules." in report
    assert "No program proves `c`." in report
