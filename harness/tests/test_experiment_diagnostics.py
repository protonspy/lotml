"""Terse against detailed diagnostics: the same refused answer, answered with each report."""

from lotml_harness.experiments import diagnostics
from lotml_harness.experiments.models import Completion, Model
from lotml_harness.tasks import Case, Task, types

TASK = Task(
    id="t/1",
    source="t",
    name="double",
    params=[("n", types.Prim("int"))],
    returns=types.Prim("int"),
    doc="Twice n.",
    tests=[Case([2], 4)],
)

REFUSED = "fn double(n: int) -> int:\n    x = n\n    x = x * 2\n    return x\n"
FIXED = "fn double(n: int) -> int:\n    var x = n\n    x = x * 2\n    return x\n"

CHECK = {
    "version": 1,
    "diagnostics": [
        {
            "code": "E0301",
            "severity": "error",
            "message": "`x` is immutable and cannot be assigned again",
            "file": "solution.lotml",
            "location": {"line": 3, "column": 5},
            "labels": [{"location": {"line": 2, "column": 5}, "message": "declared here"}],
            "notes": ["declare it `var x` if it changes"],
            "alternatives": ["n"],
            "fixes": [{"message": "declare it with `var`", "applicability": "MachineApplicable"}],
        }
    ],
}


def test_a_terse_report_keeps_where_what_and_the_alternatives():
    text = diagnostics.terse(CHECK)
    assert "solution.lotml:3:5: error[E0301]: `x` is immutable and cannot be assigned again" in text
    assert "alternatives: n" in text
    assert "declared here" not in text and "var x" not in text


def test_a_detailed_report_adds_labels_notes_fixes_and_each_code_s_page_once():
    pages = {"E0301": "E0301: an immutable reassigned\n\nDeclare it `var x = …` if it changes."}
    shown = "solution.lotml:3:5: error[E0301]: ...\n  note: declare it `var x` if it changes\n"
    text = diagnostics.detailed(shown, CHECK, pages)
    assert text.startswith(shown)
    assert text.count("E0301: an immutable reassigned") == 1


def test_the_refused_first_answers_are_taken_from_the_phase_1_runs():
    rows = [
        {"model": "m", "language": "lotml", "task": "t/1", "error": None,
         "rounds": [{"code": REFUSED, "outcome": "does not check"}]},
        {"model": "m", "language": "lotml", "task": "t/2", "error": None,
         "rounds": [{"code": "x", "outcome": "tests fail"}]},
        {"model": "m", "language": "python", "task": "t/1", "error": None,
         "rounds": [{"code": "x", "outcome": "does not check"}]},
    ]  # fmt: skip
    assert diagnostics.refused(rows) == [("m", "t/1", REFUSED)]


class Scripted(Model):
    def __init__(self, answer: str):
        super().__init__("scripted", "Test")
        self.answer = answer
        self.seen = []

    def chat(self, system, messages):
        self.seen.append([m["content"] for m in messages])
        return Completion(f"```lotml\n{self.answer}```", input_tokens=10, output_tokens=5)


def test_each_mode_answers_the_refused_answer_with_its_own_report():
    model = Scripted(FIXED)
    record = diagnostics.ask(model, TASK, REFUSED, "terse", diagnostics.Compiler())
    assert record["passed"] and record["outcome"] == "pass"
    ((user, previous, feedback),) = model.seen
    assert "```lotml\nfn double(n: int) -> int:\n" in user
    assert REFUSED in previous
    assert "error[E0301]" in feedback and "fix:" not in feedback
    model = Scripted(FIXED)
    record = diagnostics.ask(model, TASK, REFUSED, "detailed", diagnostics.Compiler())
    assert "fix:" in model.seen[0][2] and "E0301: " in model.seen[0][2]


def test_an_answer_the_compiler_now_accepts_is_not_asked_about():
    model = Scripted(FIXED)
    assert diagnostics.ask(model, TASK, FIXED, "terse", diagnostics.Compiler()) is None
    assert model.seen == []


def test_the_summary_pairs_each_answer_s_two_outcomes():
    records = [
        {"model": "m", "task": "a", "mode": "terse", "passed": True},
        {"model": "m", "task": "a", "mode": "detailed", "passed": True},
        {"model": "m", "task": "b", "mode": "terse", "passed": False},
        {"model": "m", "task": "b", "mode": "detailed", "passed": True},
        {"model": "m", "task": "c", "mode": "terse", "passed": False},
    ]
    summary = diagnostics.summarize(records)
    assert summary["m"]["pairs"] == 2
    assert summary["m"]["solved"] == {"terse": 1, "detailed": 2}
    assert (summary["m"]["only_terse"], summary["m"]["only_detailed"]) == (0, 1)
    assert summary["m"]["p"] == 1.0
