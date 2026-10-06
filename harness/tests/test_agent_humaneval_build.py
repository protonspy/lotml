"""HumanEval's problems posed as untyped agent tasks (specs/agent-humaneval/ R1.3-R1.8)."""

from pathlib import Path

import pytest

from lotml_harness.agent import humaneval
from lotml_harness.agent.grade import grade
from lotml_harness.agent.humaneval import Refused, Report, build, markdown, pose_humaneval

ADD = {
    "task_id": "HumanEval/7",
    "entry_point": "add",
    "prompt": 'def add(a, b):\n    """ Add two numbers.\n    >>> add(1, 2)\n    3\n    """\n',
    "canonical_solution": "    return a + b\n",
    "test": (
        "def check(candidate):\n    assert candidate(1, 2) == 3\n    assert candidate(-4, 4) == 0\n"
    ),
}


def test_a_problem_becomes_one_untyped_file_with_its_docstring_and_todo():
    task = pose_humaneval(ADD)
    assert task.id == "humaneval-7"
    assert task.kind == "implement"
    assert task.graded == ("solution.lotml",)
    assert task.workspace_files == {
        "solution.lotml": (
            'fn add(a, b):\n    """ Add two numbers.\n    >>> add(1, 2)\n    3\n    """\n'
            "    return todo()\n"
        )
    }
    assert task.solution_files == {}, "no reference solution: the cases are the canonical's"


def test_the_prompt_is_the_same_sentence_for_every_task():
    assert pose_humaneval(ADD).prompt == (
        "Write `add` in `solution.lotml`: give its parameters and its return their lotml types,"
        " and implement it as its docstring says.\n"
    )


def test_the_hidden_blocks_are_the_recorded_cases_and_a_typed_solution_passes_them(
    tmp_path: Path,
):
    task = pose_humaneval(ADD)
    assert task.hidden("solution.lotml").count('test "hidden:') == 2
    task.lay(tmp_path)
    assert grade(task, tmp_path).outcome == "no check", "the untyped file does not check"
    (tmp_path / "solution.lotml").write_text(
        'fn add(a: int, b: int) -> int:\n    """Add."""\n    return a + b\n', encoding="utf-8"
    )
    result = grade(task, tmp_path)
    assert (result.outcome, result.passed, result.total) == ("pass", 2, 2)


def test_a_docstring_found_after_an_import_is_kept():
    record = ADD | {"prompt": 'def add(a, b):\n    import math\n    """Add."""\n'}
    assert '"""Add."""' in pose_humaneval(record).workspace_files["solution.lotml"]


@pytest.mark.parametrize(
    ("change", "reason"),
    [
        ({"prompt": 'def add(a, b):\n    """Say ""\\"" twice."""\n'}, "docstring"),
        (
            {
                "canonical_solution": "    return [a, str(b)]\n",
                "test": "def check(candidate):\n    assert candidate(1, 2) == [1, '2']\n",
            },
            "literal",
        ),
        ({"canonical_solution": "    raise ValueError()\n"}, "record"),
        ({"prompt": "def add(*a):\n    pass\n"}, "signature"),
        ({"prompt": "def other(a, b):\n    pass\n"}, "signature"),
    ],
    ids=["triple quote", "no literal", "failing check", "variadic", "no entry point"],
)
def test_a_problem_that_cannot_be_posed_is_refused_with_its_reason(change, reason):
    with pytest.raises(Refused) as refused:
        pose_humaneval(ADD | change)
    assert refused.value.reason == reason


def test_unsatisfiable_blocks_are_refused(monkeypatch):
    monkeypatch.setattr(humaneval, "satisfiable", lambda source, lotml=None: "error[E0204]: x")
    with pytest.raises(Refused) as refused:
        pose_humaneval(ADD)
    assert refused.value.reason == "unsatisfiable"


def test_a_build_keeps_what_poses_and_counts_the_rest_by_reason():
    def posing(record):
        if record["task_id"] == "HumanEval/2":
            raise Refused("literal", "int and str have no common lotml type")
        return pose_humaneval(ADD | record)

    records = [{"task_id": "HumanEval/1"}, {"task_id": "HumanEval/2"}]
    kept, report = build(records, "abc", posing, workers=2)
    assert [t.id for t in kept] == ["humaneval-1"]
    assert report.read == 2
    assert report.cases == {"humaneval-1": 2}
    assert report.refused == {"humaneval-2": "literal: int and str have no common lotml type"}
    assert report.by_reason == {"literal": 1}


def test_the_report_carries_the_counts_the_digest_and_the_notice():
    report = Report(digest="d" * 64, read=3, cases={"humaneval-1": 4, "humaneval-10": 2})
    report.refused["humaneval-2"] = "record: timeout"
    text = markdown(report, "Agent tasks from HumanEval", "the origin", humaneval.HUMANEVAL_NOTICE)
    assert "| 3 | 2 | 1 | 6 |" in text
    assert "| record | 1 |" in text
    assert "| humaneval-2 | record: timeout |" in text
    assert text.index("| humaneval-1 | 4 |") < text.index("| humaneval-10 | 2 |")
    assert f"SHA-256 `{'d' * 64}`" in text
    assert "> Copyright (c) OpenAI (https://openai.com)" in text


def test_a_sample_is_drawn_from_the_sorted_ids_by_a_seeded_generator():
    ids = [f"humaneval-{n}" for n in (5, 1, 3, 2, 4)]
    first = humaneval.sample(ids, 3, seed=0)
    assert first == humaneval.sample(list(reversed(ids)), 3, seed=0), "the order given is moot"
    assert len(first) == 3
    assert set(first) <= set(ids)
    other = [humaneval.sample(ids, 3, seed=s) for s in range(1, 6)]
    assert any(o != first for o in other), "the seed decides the draw"
    assert humaneval.sample(ids, 10, seed=0) == humaneval.sample(ids, 5, seed=0), "at most all"
