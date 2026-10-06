"""The programs seeded failures are made from (specs/seeded-failures/ R1.1-R1.3, R3.2)."""

import json
from pathlib import Path

import pytest

from lotml_harness import split
from lotml_harness.agent import humaneval
from lotml_harness.agent.bench import tasks
from lotml_harness.agent.humaneval import Refused
from lotml_harness.guide import seeded
from lotml_harness.guide.seeded import HeldOut

ADD = {
    "task_id": "HumanEval/0",
    "entry_point": "add",
    "prompt": 'def add(a, b):\n    """Add two numbers."""\n',
    "canonical_solution": "    return a + b\n",
    "test": (
        "def check(candidate):\n    assert candidate(1, 2) == 3\n    assert candidate(-4, 4) == 0\n"
    ),
}


def test_the_benchmark_gives_every_solution_with_its_hidden_blocks_all_train():
    programs = seeded.bench_programs()
    assert len(programs) == len(tasks())
    for program, task in zip(programs, tasks(), strict=True):
        assert (program.problem, program.split, program.source) == (
            f"bench/{task.id}",
            "train",
            "bench",
        )
        for name in task.graded:
            assert program.files[name].startswith(task.solution_files[name].rstrip("\n"))
            assert program.files[name].endswith(task.hidden(name))
        assert program.prompt == task.prompt
        assert program.notice == seeded.BENCH_NOTICE


@pytest.mark.parametrize("ident", ["humaneval-4", "HumanEval_4_mean", "mbpp-2"])
def test_a_program_of_a_held_out_problem_stops_the_run(ident):
    assert split.split(split.problem(ident)) == "held-out"
    with pytest.raises(HeldOut):
        seeded.program(ident, "humaneval-original", "", {}, (), "")


def test_a_canonical_solution_is_translated_by_the_rules_typed_from_its_cases():
    code, blocks = seeded.translated(ADD)
    assert code.startswith("fn add(a: int, b: int) -> int:\n")
    assert blocks.count('test "hidden:') == 2


@pytest.mark.parametrize(
    ("change", "reason"),
    [
        (
            {
                "canonical_solution": (
                    "    def inner(x):\n        return x\n    return inner(a) + b\n"
                )
            },
            "rules",
        ),
        (
            {"canonical_solution": "    n = 0\n    n += a == b\n    return n + a + b\n"},
            "no-check",
        ),
    ],
    ids=["nested function", "does not check"],
)
def test_a_solution_the_rules_cannot_write_or_that_does_not_check_is_left_out(change, reason):
    with pytest.raises(Refused) as refused:
        seeded.translated(ADD | change)
    assert refused.value.reason == reason


def test_humaneval_takes_train_and_validation_problems_and_counts_the_rest(monkeypatch):
    held = ADD | {"task_id": "HumanEval/4"}
    untranslatable = ADD | {
        "task_id": "HumanEval/1",
        "canonical_solution": "    def inner(x):\n        return x\n    return inner(a) + b\n",
    }
    tried = []
    real = seeded.translated

    def translated(record, lotml=None):
        tried.append(record["task_id"])
        return real(record, lotml)

    monkeypatch.setattr(humaneval, "read_humaneval", lambda: ([ADD, held, untranslatable], "d"))
    monkeypatch.setattr(seeded, "translated", translated)
    taken = seeded.humaneval_programs(workers=2)
    assert [p.problem for p in taken.programs] == ["humaneval/0"]
    assert taken.programs[0].source == "humaneval-original"
    assert taken.programs[0].files["solution.lotml"].count('test "hidden:') == 2
    assert taken.programs[0].prompt == humaneval.PROMPT.format(name="add")
    assert taken.left_out == {"rules": 1}
    assert "HumanEval/4" not in tried, "a held-out problem is never translated"


def export(tmp_path: Path, bucket: str, records: list[dict]) -> Path:
    (tmp_path / bucket).mkdir(parents=True, exist_ok=True)
    lines = "".join(json.dumps(r) + "\n" for r in records)
    (tmp_path / bucket / "trajectories.jsonl").write_text(lines, encoding="utf-8")
    return tmp_path


def test_trajectories_give_their_final_files_with_their_task_s_hidden_blocks(tmp_path):
    task = next(t for t in tasks() if t.id == "median-mode")
    final = {"stats.lotml": task.solution_files["stats.lotml"]}
    record = {"messages": [], "files": final, "meta": {"task": "median-mode", "source": "bench"}}
    taken = seeded.trajectory_programs(export(tmp_path, "train", [record]), lambda _: task)
    [program] = taken.programs
    assert program.files["stats.lotml"].endswith(task.hidden("stats.lotml"))
    assert (program.problem, program.split) == ("bench/median-mode", "train")
    assert seeded.trajectory_programs(None, lambda _: task).programs == []


def test_a_trajectory_of_a_held_out_problem_stops_the_run(tmp_path):
    files = {"solution.lotml": ""}
    record = {"files": files, "meta": {"task": "humaneval-4", "source": "humaneval-original"}}
    path = export(tmp_path, "validation", [record])
    with pytest.raises(HeldOut):
        seeded.trajectory_programs(path, lambda _: tasks()[0])


def never(task_id):
    raise AssertionError("the split is checked before the task is looked up")


def test_a_held_out_trajectory_stops_before_its_task_is_looked_up(tmp_path):
    record = {"files": {"a.lotml": ""}, "meta": {"task": "mbpp-7", "source": "bench"}}
    with pytest.raises(HeldOut):
        seeded.trajectory_programs(export(tmp_path, "train", [record]), never)


def test_a_trajectory_with_an_unknown_task_or_unsafe_files_is_left_out_and_counted(tmp_path):
    task = next(t for t in tasks() if t.id == "median-mode")
    records = [
        {"files": {"a.lotml": ""}, "meta": {"task": "no-such-task", "source": "bench"}},
        {"files": {"bindings/os.lotmli": ""}, "meta": {"task": "median-mode", "source": "bench"}},
        {"files": {"a.lotml": 3}, "meta": {"task": "median-mode", "source": "bench"}},
    ]
    taken = seeded.trajectory_programs(export(tmp_path, "train", records), lambda _: task)
    assert taken.programs == []
    assert taken.left_out == {"an unknown task": 1, "unusable files: ValueError": 2}
