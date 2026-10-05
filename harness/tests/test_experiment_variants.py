import json

import pytest

from lotml_harness import reference
from lotml_harness.experiments import models, variants
from lotml_harness.lang.check import leaks
from lotml_harness.lang.grammar import parser
from lotml_harness.tasks import Case, Task
from lotml_harness.tasks.types import List, Optional, Prim

# The variant A reference ----------------------------------------------------------


def test_variant_a_reference_drops_every_construct_variant_b_took_from_python():
    text, _, excluded = variants.reference_text("a").partition("## Not in the language")
    assert "`None`, `lambda`" in excluded
    for construct in ("None", "lambda", "??", "is not none", "case ", "from math", "import math"):
        assert construct not in text, construct
    assert "none" in text and "=>" in text and "use math.{sqrt, pi}" in text
    assert "== fail Empty" in text


def test_variant_a_reference_examples_all_parse_in_variant_a():
    for example in reference.examples(variants.reference_text("a")):
        parser("a").parse(reference.program(example))


def test_variant_a_reference_keeps_the_sections_of_variant_b():
    def headings(text):
        return [line for line in text.splitlines() if line.startswith("## ")]

    assert headings(variants.reference_text("a")) == headings(variants.reference_text("b"))


def test_variant_b_reference_is_the_published_one():
    assert variants.reference_text("b") == reference.text()


def test_every_variant_a_rewrite_still_finds_its_text():
    source = reference.text()
    for old, _new in variants.A_PROSE:
        assert old in source, old


# Prompts and answers ----------------------------------------------------------------


def task() -> Task:
    return Task(
        id="humaneval/9",
        source="humaneval",
        name="first",
        params=[("xs", List(Prim("int")))],
        returns=Optional(Prim("int")),
        doc=" The first element, or None. ",
        tests=[Case([[3, 4]], 3), Case([[]], None)],
    )


def test_the_prompt_carries_the_reference_and_the_task_in_its_variant():
    system, user = variants.prompt(task(), "a")
    assert system.startswith("You write lotml") and variants.reference_text("a") in system
    assert "fn first(xs: [int]) -> int?:" in user and "or none." in user
    assert "```lotml" in user


@pytest.mark.parametrize(
    ("answer", "code"),
    [
        ("Here:\n```lotml\nfn f():\n    pass\n```\nDone.", "fn f():\n    pass\n"),
        ("```\nfn f():\n    pass\n```", "fn f():\n    pass\n"),
        (
            "```lotml\nfn a():\n    pass\n```\n```lotml\nfn b():\n    pass\n```",
            "fn b():\n    pass\n",
        ),
        ("fn f():\n    pass", "fn f():\n    pass\n"),
        ("```python\nfn f():\n    pass\n```", "fn f():\n    pass\n"),
    ],
)
def test_code_is_the_last_fenced_block_or_the_whole_answer(answer, code):
    assert variants.extract(answer) == code


# Scoring ----------------------------------------------------------------------------

GOOD_B = (
    "fn first(xs: [int]) -> int?:\n    if len(xs) == 0:\n        return None\n    return xs[0]\n"
)
GOOD_A = GOOD_B.replace("None", "none")
TRUTHY = "fn first(xs: [int]) -> int?:\n    if xs:\n        return xs[0]\n    return None\n"


def test_a_correct_answer_parses_and_passes_in_both_semantics():
    score = variants.score(task(), "b", GOOD_B)
    assert score["parse_error"] is None and score["leaks"] == [] and score["violations"] == []
    assert score["passed"] and score["passed_python"]
    assert score["cases"] == ["pass", "pass"]


def test_an_answer_relying_on_truthiness_passes_only_when_read_as_python():
    score = variants.score(task(), "b", TRUTHY)
    assert not score["passed"] and score["passed_python"]
    assert score["semantics_dependent"]


def test_a_python_answer_does_not_parse_and_leaks():
    python = "def first(xs):\n    return xs[0] if xs else None\n"
    score = variants.score(task(), "b", python)
    assert score["parse_error"] is not None
    assert score["leaks"] == ["def"]
    assert not score["passed"] and not score["passed_python"]


def test_variant_a_answers_are_scored_with_variant_a():
    assert variants.score(task(), "a", GOOD_A)["passed"]
    assert variants.score(task(), "a", GOOD_B)["leaks"] == leaks("a", GOOD_B)


# Statistics -------------------------------------------------------------------------


@pytest.mark.parametrize(
    ("b", "c", "p"),
    [
        (0, 0, 1.0),
        (5, 5, 1.0),
        (0, 6, 0.03125),
        (2, 10, 0.0386),
        (1, 1, 1.0),
    ],
)
def test_mcnemar_exact_is_the_two_sided_binomial_on_discordant_pairs(b, c, p):
    assert variants.mcnemar(b, c) == pytest.approx(p, abs=1e-4)


# Running and resuming -----------------------------------------------------------------


class Scripted(models.Model):
    def __init__(self, answers: dict[str, str]):
        super().__init__(name="scripted", family="test")
        self.answers = answers
        self.calls = 0

    def complete(self, system: str, user: str) -> models.Completion:
        self.calls += 1
        variant = "a" if "none" in user else "b"
        return models.Completion(text=self.answers[variant], input_tokens=10, output_tokens=5)


def test_run_scores_every_task_in_both_variants_and_resumes(tmp_path):
    model = Scripted({"a": f"```lotml\n{GOOD_A}```", "b": f"```lotml\n{TRUTHY}```"})
    path = tmp_path / "runs.jsonl"
    rows = variants.run(model, [task()], path)
    assert [(r["variant"], r["passed"]) for r in rows] == [("a", True), ("b", False)]
    assert model.calls == 2
    again = variants.run(model, [task()], path)
    assert model.calls == 2 and again == rows
    stored = [json.loads(line) for line in path.read_text().splitlines()]
    assert {r["model"] for r in stored} == {"scripted"}


def test_a_failed_completion_is_recorded_and_retried_on_the_next_run(tmp_path):
    class Flaky(Scripted):
        def complete(self, system, user):
            self.calls += 1
            if self.calls == 1:
                raise models.ModelError("rate limited")
            return super().complete(system, user)

    model = Flaky({"a": GOOD_A, "b": GOOD_B})
    path = tmp_path / "runs.jsonl"
    rows = variants.run(model, [task()], path)
    assert [r.get("error") for r in rows] == ["rate limited", None]
    rows = variants.run(model, [task()], path)
    assert [r.get("error") for r in rows] == [None, None]


def test_sample_is_seeded_stratified_and_paired():
    tasks = [
        Task(f"{s}/{i}", s, "f", [], Prim("int"), "", [])
        for s in ("humaneval", "mbpp")
        for i in range(10)
    ]
    first = variants.sample(tasks, {"humaneval": 3, "mbpp": 2}, seed=1)
    assert [t.source for t in first].count("humaneval") == 3
    assert first == variants.sample(tasks, {"humaneval": 3, "mbpp": 2}, seed=1)
    assert first != variants.sample(tasks, {"humaneval": 3, "mbpp": 2}, seed=2)


# The report -------------------------------------------------------------------------


def row(model, variant, task_id, passed, **extra):
    return {
        "model": model,
        "family": "f",
        "variant": variant,
        "task": task_id,
        "parse_error": extra.get("parse_error"),
        "leaks": extra.get("leaks", []),
        "violations": extra.get("violations", []),
        "passed": passed,
        "passed_python": extra.get("passed_python", passed),
        "semantics_dependent": extra.get("semantics_dependent", False),
        "output_tokens": 5,
    }


def test_summary_counts_parse_leaks_passes_and_discordant_pairs():
    rows = [
        row("m", "a", "t1", True),
        row("m", "b", "t1", False),
        row("m", "a", "t2", False, parse_error="x", leaks=["def"]),
        row("m", "b", "t2", True, semantics_dependent=True, passed_python=False),
        row("m", "a", "t3", False),
        row("m", "b", "t3", True),
    ]
    summary = variants.summarize(rows)["m"]
    assert summary["a"]["parsed"] == 2 and summary["a"]["leaked"] == 1
    assert summary["b"]["passed"] == 2 and summary["b"]["semantics_dependent"] == 1
    assert summary["pairs"] == 3 and (summary["a_only"], summary["b_only"]) == (1, 2)
    text = variants.markdown(rows, {"m": "Claude"})
    assert "| m | Claude | 3 |" in text
    assert "McNemar" in text


def test_collected_keeps_each_answer_once_and_only_for_the_sample(tmp_path):
    lines = [
        {"model": "m", "task": "t1", "variant": "a", "error": "quota"},
        {"model": "m", "task": "t1", "variant": "a", "error": None, "passed": True},
        {"model": "m", "task": "t9", "variant": "a", "error": None},
    ]
    (tmp_path / "m.jsonl").write_text("\n".join(json.dumps(x) for x in lines) + "\n")
    sampled = [Task("t1", "s", "f", [], Prim("int"), "", [])]
    assert variants.collected(tmp_path, sampled) == [lines[1]]


def test_refresh_recomputes_the_in_process_checks(tmp_path):
    stale = {"model": "m", "variant": "b", "task": "t", "code": GOOD_B, "error": None}
    stale |= {"parse_error": "spurious", "leaks": [], "violations": [], "passed": True}
    failed = {"model": "m", "variant": "b", "task": "u", "error": "quota"}
    path = tmp_path / "m.jsonl"
    path.write_text(json.dumps(stale) + "\n" + json.dumps(failed) + "\n")
    assert variants.refresh(path) == 1
    first, second = [json.loads(line) for line in path.read_text().splitlines()]
    assert first["parse_error"] is None and first["passed"] is True
    assert second == failed
    assert variants.refresh(path) == 0
