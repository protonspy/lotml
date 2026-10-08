"""The two-target differential fuzzer: generated programs and corpus mutants on the Python target,
the oracle, and on the LLVM target, compared by output, ending and kind of error."""

import json
import random
import re

from lotml_harness.experiments import differential

Run = differential.Run


def test_the_templates_generate_no_import_and_no_file_access():
    rng = random.Random(0)  # noqa: S311 - a reproducible stream of cases, not a secret
    cases = [differential.case(rng) for _ in range(2000)]
    banned = re.compile(r"\b(import|from|open|read|write|path|os|sys)\b")
    assert not [c for c in cases if banned.search(c)]


def test_a_program_calls_each_case_in_order_and_lines_of_names_their_lines():
    cases = ["print(1)", "xs = [1]\n    print(xs[0])"]
    text = differential.program(cases)
    lines = text.splitlines()
    spans = differential.lines_of(cases)
    assert lines[spans[0].start - 1] == "fn case_0():"
    assert lines[spans[1].start - 1] == "fn case_1():"
    assert lines[spans[1].stop - 2] == "    print(xs[0])"
    assert text.endswith("fn main():\n    case_0()\n    case_1()\n")


def test_the_same_output_and_ending_is_agreement_and_a_panic_is_compared_by_its_kind():
    same = differential.verdict(Run("1\n", 0, "", None), Run("1\n", 0, "", 0))
    assert same.same
    panics = differential.verdict(
        Run("1\n", 101, "ValueError", None), Run("1\n", 101, "ValueError", None)
    )
    assert panics.same, "the messages may differ: only the kind is compared"
    kinds = differential.verdict(Run("", 101, "ValueError", None), Run("", 101, "Overflow", None))
    assert not kinds.same and "ValueError" in kinds.detail and "Overflow" in kinds.detail


def test_a_different_output_names_the_first_line_that_differs():
    found = differential.verdict(Run("1\n2\n3\n", 0, "", None), Run("1\n2\n4\n", 0, "", 0))
    assert not found.same
    assert found.detail == "line 3: Python ['3'], LLVM ['4']"


def test_a_native_program_that_leaves_cells_differs_even_when_it_prints_the_same():
    found = differential.verdict(Run("1\n", 0, "", None), Run("1\n", 0, "", 3))
    assert not found.same and found.detail == "3 cells live at exit"


def test_a_run_past_its_limit_differs_unless_both_were():
    late = Run("", None, "", None)
    assert not differential.verdict(Run("", 0, "", None), late).same
    assert differential.verdict(late, late).same


def test_minimizing_keeps_only_what_the_difference_needs():
    text = "a\nb\nbad\nc\nd\ne\n"
    assert differential.minimize(text, lambda t: "bad" in t) == "bad\n"
    pair = "x\nneed1\ny\nz\nneed2\nw\n"
    assert differential.minimize(pair, lambda t: "need1" in t and "need2" in t) == "need1\nneed2\n"


def test_a_finding_is_written_as_a_program_saying_what_differed(monkeypatch, tmp_path):
    monkeypatch.setattr(differential, "FOUND", tmp_path)
    finding = differential.Finding(
        "generated", "line 1: Python ['1'],\nLLVM []", "fn main():\n    print(1)\n"
    )
    path = differential.write_finding(0, 7, finding)
    assert path == tmp_path / "seed7-0.lot"
    header, program = path.read_text(encoding="utf-8").split("\n", 1)
    assert header == "# differential, generated, seed 7: line 1: Python ['1'], LLVM []"
    assert program == "fn main():\n    print(1)\n"


def test_the_native_build_is_sanitized_and_counts_its_cells_and_python_prints_utf8():
    native = differential.native_environment()
    assert native["LOTML_SANITIZE"] == "1" and native["LOTML_COUNT_CELLS"] == "1"
    assert differential.python_environment()["PYTHONUTF8"] == "1"


def test_a_shared_panic_is_agreement_and_the_cases_after_it_still_run():
    cases = ["print(1 // 0)", "print(2)", "print(-1.5 ** 0.5)"]
    assert differential.batch(cases) is None


def test_mutant_reports_are_compared_by_the_kind_of_error_not_its_words_or_frames():
    def report(message: str, trace: list) -> differential.subprocess.CompletedProcess:
        row = {"name": "t", "outcome": "panic", "kind": "IndexError", "line": 7}
        row |= {"message": message, "trace": trace, "file": "case.lot"}
        out = json.dumps({"version": 1, "tests": [row]}) + "\n"
        return differential.subprocess.CompletedProcess([], 1, out, "")

    python = report("list index out of range", [{"function": "__test_1"}, {"function": "f"}])
    native = report("index 9 is out of range", [{"function": "f"}])
    outcome = differential.parity.compare(
        "m", differential.by_kind(python), differential.by_kind(native)
    )
    assert outcome.verdict == "same"
