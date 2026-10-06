"""The phase 1 experiment: prompts, feedback, the repair loop, the summary and the gate."""

import pytest

from lotml_harness.execute import Result, isolated
from lotml_harness.experiments import phase1
from lotml_harness.experiments.models import Completion, Model
from lotml_harness.tasks import Case, Task, types

TASK = Task(
    id="t/1",
    source="t",
    name="add",
    params=[("a", types.Prim("int")), ("xs", types.List(types.Prim("f64")))],
    returns=types.Optional(types.Prim("str")),
    doc="Add things.",
    tests=[Case([1, [2.0]], "3"), Case([0, []], None), Case([2, [1.0]], "x")],
)


def test_python_types_are_written_as_python_annotates_them():
    assert phase1.python_type(types.Optional(types.List(types.Prim("f64")))) == "list[float] | None"
    assert phase1.python_type(types.Dict(types.Prim("str"), types.Tuple((types.Prim("int"),)))) == (
        "dict[str, tuple[int]]"
    )


def test_the_python_prompt_is_the_typed_signature_and_docstring():
    system, user = phase1.python_prompt(TASK)
    assert "Python" in system
    assert 'def add(a: int, xs: list[float]) -> str | None:\n    """Add things."""\n' in user


def test_test_feedback_shows_the_value_returned_in_the_program_s_language():
    result = Result(
        cases=["pass", "wrong answer", "ZeroDivisionError"],
        observed={"1": '"0"'},
        tracebacks={"2": 'File "solution.lotml", line 3\nZeroDivisionError: division by zero'},
    )
    feedback = phase1.test_feedback(TASK, result, "lotml")
    assert feedback.startswith("2 of 3 hidden tests fail:")
    assert '`add(0, [])` returned `"0"`; expected `None`.' in feedback
    assert "`add(2, [1.0])` stopped: ZeroDivisionError: division by zero." in feedback
    python = phase1.test_feedback(TASK, result, "python")
    assert "`add(0, [])` returned" in python and "expected `None`" in python


class Scripted(Model):
    """A model answering from a list, recording what it was asked."""

    def __init__(self, answers):
        super().__init__("scripted", "Test")
        self.answers = list(answers)
        self.seen = []

    def chat(self, system, messages):
        self.seen.append([m["content"] for m in messages])
        return Completion(self.answers.pop(0), input_tokens=10, output_tokens=5, seconds=0.1)


class Judge:
    """An arm that passes the code `good`."""

    def prompt(self, task):
        return "system", "write it"

    def judge(self, task, code):
        if code.strip() == "good":
            return phase1.Verdict(True, "pass", "")
        return phase1.Verdict(False, "tests fail", f"feedback on {code.strip()}")


def test_the_loop_answers_again_after_feedback_until_green():
    model = Scripted(["```\nbad\n```", "```\ngood\n```"])
    record = phase1.converse(model, Judge(), TASK, "lotml")
    assert record["green"] == 2
    assert [r["outcome"] for r in record["rounds"]] == ["tests fail", "pass"]
    assert "feedback on bad" in model.seen[1][-1]


def test_the_loop_stops_after_the_last_round():
    record = phase1.converse(Scripted(["bad"] * phase1.ROUNDS), Judge(), TASK, "python")
    assert record["green"] is None and len(record["rounds"]) == phase1.ROUNDS


def row(task, language, green, code="x = 1\n"):
    rounds = [{"code": code, "outcome": "pass", "output_tokens": 5}] * (green or phase1.ROUNDS)
    return {
        "model": "m",
        "family": "F",
        "task": task,
        "language": language,
        "green": green,
        "rounds": rounds,
    }


def test_the_summary_pairs_tasks_and_the_gate_reads_it():
    rows = []
    for i in range(phase1.PAIRS_NEEDED):
        rows += [
            row(f"t{i}", "lotml", 1, "a\n"),
            row(f"t{i}", "python", 1 if i % 2 else 2, "a b\n"),
        ]
    summary = phase1.summarize(rows)["m"]
    assert summary["pairs"] == phase1.PAIRS_NEEDED
    assert summary["only_lotml"] == phase1.PAIRS_NEEDED // 2 and summary["only_python"] == 0
    assert summary["median_rounds"] == 1
    assert summary["token_ratio"] < 1
    assert all(c.passed for c in phase1.gate({"m": summary}))


def test_the_report_says_how_each_first_answer_ended_and_why_lotml_refused_it():
    refused = {
        "code": "x\n",
        "outcome": "does not check",
        "output_tokens": 5,
        "feedback": (
            "`lotml check` reports:\n\n"
            "s.lotml:1:1: error[E0201]: no `y`\n"
            "s.lotml:2:1: error[E0204]: …"
        ),
    }
    wrong = {"code": "x\n", "outcome": "tests fail", "output_tokens": 5, "feedback": "…"}
    rows = [
        row("a", "lotml", None) | {"rounds": [refused]},
        row("b", "lotml", None) | {"rounds": [refused]},
        row("c", "lotml", None) | {"rounds": [wrong]},
        row("a", "python", 1),
        row("b", "python", None) | {"rounds": [wrong]},
        row("c", "python", 1),
    ]
    summary = phase1.summarize(rows)
    first = summary["m"]["first"]
    assert first["lotml"]["does not check"] == 2 and first["lotml"]["tests fail"] == 1
    assert first["python"]["pass"] == 2 and first["python"]["tests fail"] == 1
    # Only the first error of each refused answer counts: what the model met first.
    assert summary["m"]["codes"] == [("E0201", 2)]
    text = phase1.markdown(summary, phase1.gate(summary))
    assert "| m | lotml | 0 | 2 | 1 | 0 | E0201 2 |" in text
    assert "| m | python | 2 | 0 | 1 | 0 | — |" in text


def test_too_few_pairs_do_not_pass_the_gate():
    summary = phase1.summarize([row("t", "lotml", 1), row("t", "python", 1)])
    assert not phase1.gate(summary)[0].passed


@pytest.mark.parametrize(
    ("code", "outcome"),
    [
        ("def add(a, xs):\n    return str(a + int(sum(xs)))\n", "pass"),
        ("import os\ndef add(a, xs):\n    return None\n", "does not run"),
        ("def add(a, xs):\n    return str(a)\n", "tests fail"),
    ],
)
def test_a_python_answer_is_run_on_the_hidden_tests(code, outcome):
    task = Task(**{**TASK.__dict__, "tests": [Case([1, [2.0]], "3")]})
    assert phase1.Python().judge(task, code).outcome == outcome


def test_an_isolated_solution_reports_what_it_returned():
    task = Task(**{**TASK.__dict__, "tests": [Case([1, [2.0]], "3")]})
    result = isolated("def add(a, xs):\n    return 'no'\n", mode="solution", task=task)
    assert result.cases == ["wrong answer"] and result.observed == {"0": "'no'"}


ONE_CASE = Task(**{**TASK.__dict__, "tests": [Case([1, [2.0]], "3")]})


def test_a_lotml_answer_is_checked_then_run_on_the_hidden_tests():
    lotml = phase1.Lotml()
    head = "fn add(a: int, xs: [f64]) -> str?:\n"
    assert lotml.judge(ONE_CASE, head + "    return str(a + int(sum(xs)))\n").outcome == "pass"
    refused = lotml.judge(ONE_CASE, head + "    x = 1\n    x = 2\n    return None\n")
    assert refused.outcome == "does not check"
    assert "solution.lotml:3:5: error[E0301]" in refused.feedback
    wrong = lotml.judge(ONE_CASE, head + '    return "0"\n')
    assert wrong.outcome == "tests fail"
    assert '`add(1, [2.0])` returned `"0"`; expected `"3"`.' in wrong.feedback
