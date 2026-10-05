import json

import pytest

from lotml_harness import reference
from lotml_harness.experiments import editing, models
from lotml_harness.lang.braces import from_braces, to_braces
from lotml_harness.lang.grammar import parser
from lotml_harness.tasks import Case, Task
from lotml_harness.tasks.types import List, Prim

POSITIVE = (
    "fn count_positive(xs: [int]) -> int:\n"
    "    var n = 0\n"
    "    for x in xs:\n"
    "        if x > 0:\n"
    "            n += 1\n"
    "    return n\n"
)
DOUBLE = "fn double(x: int) -> int:\n    return x * 2\n"
HALF = "type Pair(a: int, b: int)\n\nfn half(x: int) -> int:\n    return x // 2\n"


def task() -> Task:
    return Task(
        id="t/1",
        source="t",
        name="count_positive",
        params=[("xs", List(Prim("int")))],
        returns=Prim("int"),
        doc="",
        tests=[Case([[1, -2, 3]], 2), Case([[-1]], 0), Case([[]], 0)],
    )


# Building tasks ----------------------------------------------------------------------


def test_solutions_take_passing_variant_b_answers_by_writer_preference(tmp_path):
    rows = {
        "claude-haiku": [
            {"task": "t/1", "variant": "b", "passed": True, "violations": [], "code": "haiku"},
            {"task": "t/2", "variant": "b", "passed": True, "violations": [], "code": "h2"},
            {"task": "t/3", "variant": "b", "passed": True, "violations": ["x"], "code": "bad"},
        ],
        "claude-sonnet": [
            {"task": "t/1", "variant": "b", "passed": True, "violations": [], "code": "sonnet"},
            {"task": "t/2", "variant": "a", "passed": True, "violations": [], "code": "a"},
            {"task": "t/4", "variant": "b", "passed": False, "violations": [], "code": "no"},
        ],
    }
    rows["qwen"] = [{"task": "t/5", "variant": "b", "passed": True, "code": "not a writer"}]
    for writer, lines in rows.items():
        stored = [r | {"model": writer} for r in lines]
        (tmp_path / f"{writer}.jsonl").write_text("\n".join(json.dumps(r) for r in stored))
    tasks = {f"t/{i}": task() for i in range(1, 6)}
    assert editing.solutions(tasks, tmp_path) == {"t/1": "sonnet", "t/2": "h2"}


def test_more_solutions_answer_unsampled_tasks_in_variant_b(monkeypatch, tmp_path):
    seen = {}

    def answer(model, tasks, path, workers, variants):
        seen.update(model=model.name, count=len(tasks), path=path.name, variants=variants)

    monkeypatch.setattr(editing, "answer", answer)
    monkeypatch.setattr(editing, "ANSWERS", tmp_path)
    editing.more_solutions("claude:sonnet", 5)
    assert seen == {
        "model": "claude-sonnet",
        "count": 5,
        "path": "extra-claude-sonnet.jsonl",
        "variants": ("b",),
    }


def test_same_program_ignores_layout_but_not_meaning():
    assert editing.same_program("fn f():\n    return 1", "fn f():\n        return 1")
    assert not editing.same_program("fn f():\n    return 2", "fn f():\n    return 1")
    assert not editing.same_program(None, "fn f():\n    return 1")
    assert not editing.same_program("fn f(", "fn f(")


def test_top_level_names_cover_types_and_functions():
    assert editing.top_level_names(HALF) == {"Pair", "half"}


def test_assemble_avoids_clashing_names_and_reaches_the_length(monkeypatch):
    monkeypatch.setattr(editing, "FILE_LINES", 9)
    whole, index = editing.assemble(POSITIVE, [POSITIVE, DOUBLE, HALF, DOUBLE], "s")
    assert whole.count("fn count_positive") == 1 and whole.count("fn double") == 1
    assert "fn half" in whole
    parser("b").parse(whole)
    pieces = whole.split("\n\n\n")
    assert pieces[index].startswith("fn count_positive")


def test_a_slip_breaks_the_function_silently():
    broken, (row, delta), failure = editing.slipped(task(), POSITIVE, "seed")
    parser("b").parse(broken)
    assert broken != POSITIVE and delta in (-1, 1) and row > 0
    assert "count_positive(" in failure and "should return" in failure


def test_function_lines_find_the_body():
    program = DOUBLE + "\n\n" + POSITIVE
    assert editing.function_lines(program, "count_positive") == (4, 9)


def test_make_tasks_builds_one_task_per_breakable_solution(monkeypatch):
    monkeypatch.setattr(editing, "FILE_LINES", 5)
    made = editing.make_tasks({"t/1": task()}, {"t/1": POSITIVE})
    assert len(made) == 1
    assert made[0].original == POSITIVE and made[0].broken != POSITIVE
    assert from_braces(to_braces(made[0].broken)) == made[0].broken


def test_tasks_round_trip_through_their_file(tmp_path):
    made = [
        editing.EditTask(
            task(), POSITIVE, POSITIVE.replace("    return n", "return n"), (5, -1), "x"
        )
    ]
    path = tmp_path / "tasks.jsonl"
    editing.save_tasks(made, path)
    assert editing.load_tasks({"t/1": task()}, path) == made


# The two forms -------------------------------------------------------------------------


def test_the_braces_reference_has_braced_examples_and_no_colon_blocks():
    text = editing.reference_text("braces")
    assert "Indentation is not significant" in text
    assert "fn area(width: f64, height: f64) -> f64 {" in text
    assert editing.reference_text("indented") == reference.text()


def test_a_braces_file_means_the_indented_program():
    braced = editing.in_form(POSITIVE, "braces")
    assert "{" in braced and editing.to_program(braced, "braces") == POSITIVE
    assert editing.to_program(POSITIVE, "indented") == POSITIVE
    assert editing.to_program("fn f() {\n", "braces") is None


# Applying edits --------------------------------------------------------------------------


def blocks(search: str, replace: str) -> str:
    return f"{editing.SEARCH}\n{search}{editing.DIVIDER}\n{replace}{editing.REPLACE}\n"


BROKEN = POSITIVE.replace("    return n\n", "        return n\n")
FIX = blocks("        return n\n", "    return n\n")


def test_apply_replaces_an_exact_match_once():
    answer = FIX
    edited, reason = editing.apply(BROKEN, editing.parse_blocks(answer))
    assert reason is None and edited == POSITIVE


@pytest.mark.parametrize(
    ("answer", "reason"),
    [
        ("no blocks here", "no edit"),
        (blocks("\n", "x\n"), "empty search"),
        (blocks("return n\n", "x\n"), "not found: whitespace"),
        (blocks("missing line\n", "x\n"), "not found"),
    ],
)
def test_apply_says_why_an_edit_failed(answer, reason):
    assert editing.apply(BROKEN, editing.parse_blocks(answer)) == (None, reason)


def test_apply_refuses_an_ambiguous_search():
    text = "a\nb\na\n"
    assert editing.apply(text, editing.parse_blocks(blocks("a\n", "c\n"))) == (None, "ambiguous")


def test_tolerant_application_shifts_by_one_offset():
    answer = blocks("if x > 0:\n    n += 1\n", "if x > 0:\n    n += 1\n    n += 0\n")
    assert editing.apply(POSITIVE, editing.parse_blocks(answer))[1] == "not found: whitespace"
    edited, reason = editing.apply(POSITIVE, editing.parse_blocks(answer), tolerant=True)
    assert reason is None and "            n += 0\n" in edited


def test_idioms_find_c_family_constructs_in_replacements():
    answer = editing.parse_blocks(blocks("x\n", "} else if (a && !b) {\n    y = 1;\n}\n"))
    assert editing.idioms(answer, "indented") == [
        "else if",
        "&&",
        "! not",
        "semicolon",
        "brace block",
    ]
    assert "brace block" not in editing.idioms(answer, "braces")
    clean = editing.parse_blocks(blocks("x\n", "if a != b and not c:\n    y = 1\n"))
    assert editing.idioms(clean, "indented") == []


# Judging and solving --------------------------------------------------------------------------


def edit_task() -> editing.EditTask:
    return editing.EditTask(
        task(),
        POSITIVE,
        BROKEN,
        (5, 1),
        "`count_positive([1, -2, 3])` should return `2`, but it does not.",
    )


@pytest.mark.parametrize("form", editing.FORMS)
def test_a_correct_fix_passes_in_either_form(form):
    current = editing.in_form(BROKEN, form)
    fixed = editing.in_form(POSITIVE, form)
    answer = blocks(current, fixed)
    attempt = editing.judge(edit_task(), form, current, answer, tolerant=False)
    assert attempt.outcome == "pass"


def test_judge_reports_syntax_and_failing_tests():
    attempt = editing.judge(
        edit_task(), "indented", BROKEN, blocks("        return n\n", "        return (\n"), False
    )
    assert attempt.outcome == "syntax" and "does not parse" in attempt.feedback
    attempt = editing.judge(
        edit_task(), "indented", BROKEN, blocks("        return n\n", "    return 7\n"), False
    )
    assert attempt.outcome == "tests fail" and "should return" in attempt.feedback
    braced = editing.in_form(BROKEN, "braces")
    attempt = editing.judge(
        edit_task(), "braces", braced, blocks("        return n\n", "        return n\n}\n"), False
    )
    assert attempt.outcome == "syntax"


class Scripted(models.Model):
    def __init__(self, answers):
        super().__init__(name="scripted", family="test")
        self.answers = list(answers)
        self.seen = []

    def chat(self, system, messages):
        self.seen.append(messages)
        return models.Completion(text=self.answers.pop(0), input_tokens=1, output_tokens=1)


def test_solve_gives_feedback_until_the_fix_passes():
    wrong = blocks("missing\n", "x\n")
    model = Scripted([wrong, FIX])
    record = editing.solve(model, edit_task(), "indented")
    assert [t["outcome"] for t in record["turns"]] == ["apply failed: not found", "pass"]
    assert record["fixed"] and not record["fixed_first"]
    assert "does not match" in model.seen[1][-1]["content"]


def test_solve_stops_after_the_last_turn_and_records_a_failed_completion():
    model = Scripted([blocks("missing\n", "x\n")] * editing.TURNS)
    record = editing.solve(model, edit_task(), "indented")
    assert len(record["turns"]) == editing.TURNS and not record["fixed"]

    class Down(models.Model):
        def chat(self, system, messages):
            raise models.ModelError("quota")

    record = editing.solve(Down("down", "x"), edit_task(), "braces")
    assert record["error"] == "quota"


def test_run_resumes_and_the_report_pairs_the_forms(tmp_path):
    right = {
        "indented": FIX,
        "braces": blocks(editing.in_form(BROKEN, "braces"), editing.in_form(POSITIVE, "braces")),
    }

    class ByForm(Scripted):
        def chat(self, system, messages):
            form = "braces" if "not significant" in system else "indented"
            return models.Completion(text=right[form] if form == "indented" else "nothing")

    path = tmp_path / "runs.jsonl"
    model = ByForm([])
    rows = editing.run(model, [edit_task()], path)
    assert [(r["form"], r["fixed"]) for r in rows] == [("indented", True), ("braces", False)]
    assert editing.run(model, [edit_task()], path) == rows
    summary = editing.summarize(rows)["scripted"]
    assert summary["pairs"] == 1 and summary["indented_only"] == 1
    text = editing.markdown(rows, made=1)
    assert "| scripted | test | 1 |" in text and "no edit" in text
