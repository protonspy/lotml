"""The parity suite: every corpus program's `test` blocks on the Python and the LLVM target, which
must report the same."""

import json
import subprocess

from lotml_harness.experiments import parity

RIGHT = 'fn double(n: int) -> int:\n    return n * 2\n\ntest "double":\n    assert double(2) == 4\n'
WRONG = 'fn double(n: int) -> int:\n    return n + 2\n\ntest "double":\n    assert double(2) == 5\n'


def ran(stdout: str, code: int = 0) -> subprocess.CompletedProcess:
    return subprocess.CompletedProcess([], code, stdout, "")


def report(*tests: dict) -> str:
    rows = [{**t, "file": "p0.lotml"} for t in tests]
    return json.dumps({"version": 1, "tests": rows, "summary": {}}) + "\n"


PASS = {"name": "double", "outcome": "pass"}
FAIL = {"name": "double", "outcome": "fail", "expression": "double(2) == 5", "line": 5}


def test_the_same_report_on_both_targets_is_the_same_outcome():
    outcome = parity.compare("t/1", ran(report(PASS)), ran(report(PASS)))
    assert (outcome.task, outcome.verdict) == ("t/1", "same")


def test_where_a_file_was_written_does_not_count():
    other = report(PASS).replace("p0.lotml", "elsewhere/p0.lotml")
    assert parity.compare("t/1", ran(report(PASS)), ran(other)).verdict == "same"


def test_a_different_outcome_is_named_with_both_rows():
    outcome = parity.compare("t/1", ran(report(PASS)), ran(report(FAIL), 1))
    assert outcome.verdict == "differs"
    assert '"outcome": "pass"' in outcome.detail and '"outcome": "fail"' in outcome.detail


def test_a_python_import_refused_on_the_llvm_target_is_set_apart():
    refused = "prog.lotml:1:1: error[E0401]: `textwrap` is a Python module, and …\n"
    assert parity.compare("t/1", ran(report(PASS)), ran(refused, 1)).verdict == "refused"


def test_a_program_the_llvm_backend_does_not_compile_names_why():
    unsupported = "prog.lotml:3:5: error[E0402]: `--target llvm` does not compile this yet\n"
    outcome = parity.compare("t/1", ran(report(PASS)), ran(unsupported, 1))
    assert outcome.verdict == "not compiled"
    assert "E0402" in outcome.detail


def test_a_native_program_that_did_not_report_is_a_failure_of_its_own():
    assert parity.compare("t/1", ran(report(PASS)), ran("", 2)).verdict == "no report"


def test_a_real_program_reports_the_same_on_both_targets_even_when_it_fails():
    outcomes = parity.suite([{"task": "t/1", "lotml": RIGHT}, {"task": "t/2", "lotml": WRONG}])
    assert [(o.task, o.verdict) for o in outcomes] == [("t/1", "same"), ("t/2", "same")]


def test_the_gate_passes_only_when_every_program_not_refused_reports_the_same():
    same = parity.Outcome("t/1", "same", "")
    refused = parity.Outcome("t/2", "refused", "")
    differs = parity.Outcome("t/3", "differs", "a row")
    assert "The suite passes" in parity.markdown([same, refused])
    assert "The suite does not pass" in parity.markdown([same, differs])
