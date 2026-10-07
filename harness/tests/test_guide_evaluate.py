"""The guide's offline evaluation on held-out real failures (specs/guide-evaluation/)."""

import json
from pathlib import Path

import pytest

from lotml_harness.guide import evaluate
from lotml_harness.guide.evaluate import Refused
from lotml_harness.tasks import Case, Task
from lotml_harness.tasks.types import Prim

BROKEN = "fn add(a: int, b: int) -> int:\n    return a + bs\n"
WRONG = "fn add(a: int, b: int) -> int:\n    return a - b\n"
FIXED = "fn add(a: int, b: int) -> int:\n    return a + b\n"


def task(ident: str) -> Task:
    return Task(
        id=ident,
        source=ident.split("/")[0],
        name="add",
        params=[("a", Prim("int")), ("b", Prim("int"))],
        returns=Prim("int"),
        doc="Add.",
        tests=[Case([1, 2], 3)],
    )


def conversation(problem: str, *rounds: tuple[str, str], model="m", language="lotml") -> dict:
    return {
        "model": model,
        "language": language,
        "task": problem,
        "rounds": [{"code": code, "outcome": outcome} for code, outcome in rounds],
    }


TASKS = {ident: task(ident) for ident in ("mbpp/2", "humaneval/0", "humaneval/4")}


def test_the_phase_1_gate_gives_a_failure_followed_by_a_passing_answer_on_a_held_out_problem():
    rows = [
        conversation("mbpp/2", (BROKEN, "does not check"), (FIXED, "pass")),
        conversation("humaneval/4", (WRONG, "tests fail"), (FIXED, "pass"), model="n"),
        conversation("humaneval/0", (BROKEN, "does not check"), (FIXED, "pass")),
        conversation("mbpp/2", (BROKEN, "does not check"), (FIXED, "pass"), language="python"),
        conversation("mbpp/2", (BROKEN, "does not check"), (BROKEN, "does not check")),
    ]
    found, refused = evaluate.phase1_failures(rows, TASKS)
    assert [(f.problem, f.kind, f.model, f.origin) for f in found] == [
        ("mbpp/2", "check", "m", "phase1"),
        ("humaneval/4", "test", "n", "phase1"),
    ]
    assert refused == {"not held out": 1}
    check, test = found
    assert (check.before, check.after, check.blocks) == (BROKEN, FIXED, "")
    assert 'test "hidden: 1":' in test.blocks and "assert add(1, 2) == 3" in test.blocks


def test_the_same_failing_file_on_one_problem_counts_once():
    rows = [conversation("mbpp/2", (BROKEN, "does not check"), (FIXED, "pass"))] * 2
    found, _ = evaluate.phase1_failures(rows, TASKS)
    assert len(found) == 1


def trace(task_id: str, model: str = "glm") -> dict:
    def check(text: str, errors: int) -> dict:
        diagnostics = [{"file": "a.lotml", "severity": "error", "code": "E0201"}] * errors
        report = {"diagnostics": diagnostics, "summary": {"errors": errors}}
        files = {"a.lotml": text}
        return {"paths": [], "before": files, "after": files, "report": report,
                "status": "success", "truncated": False}  # fmt: skip

    return {
        "row": {"task": task_id, "model": model, "arm": "agents", "outcome": "pass"},
        "messages": [{"type": "human", "data": {"content": "Write add."}}],
        "checks": [check(BROKEN, 1), check(FIXED, 0)],
        "tests": [],
    }


def test_agent_traces_give_their_repairs_on_held_out_problems_and_refuse_the_rest(tmp_path: Path):
    for name, body in (("held.json", trace("mbpp-2")), ("train.json", trace("humaneval-0"))):
        (tmp_path / name).write_text(json.dumps(body), encoding="utf-8")
    found, refused = evaluate.trace_failures(tmp_path)
    [failure] = found
    assert (failure.problem, failure.origin, failure.model, failure.kind) == (
        "mbpp/2",
        "agent",
        "glm",
        "check",
    )
    assert (failure.before, failure.after, failure.task) == (BROKEN, FIXED, "Write add.")
    assert refused == {"not held out": 1}


def records(directory: Path, *metas: dict) -> Path:
    directory.mkdir(parents=True, exist_ok=True)
    lines = "".join(json.dumps({"messages": [], "meta": m}) + "\n" for m in metas)
    (directory / "train.jsonl").write_text(lines, encoding="utf-8")
    return directory


def test_the_guard_reads_every_training_record_and_returns_their_digest(tmp_path: Path):
    good = records(tmp_path / "good", {"problem": "humaneval/0", "split": "train"})
    assert len(evaluate.guard(good)) == 64


@pytest.mark.parametrize(
    "metas",
    [
        (),
        ({"split": "train"},),
        ({"problem": "humaneval/0"},),
        ({"problem": "humaneval/4", "split": "train"},),
        ({"problem": "mbpp/2", "split": "held-out"},),
    ],
    ids=["empty", "no problem", "no split", "held out by the split", "marked held out"],
)
def test_the_guard_refuses_records_that_could_leak_the_evaluation(tmp_path: Path, metas):
    with pytest.raises(Refused):
        evaluate.guard(records(tmp_path / "records", *metas))


def test_the_guard_refuses_records_that_are_missing_or_unreadable(tmp_path: Path):
    with pytest.raises(Refused):
        evaluate.guard(None)
    with pytest.raises(Refused):
        evaluate.guard(tmp_path / "nowhere")
    (tmp_path / "bad").mkdir()
    (tmp_path / "bad" / "train.jsonl").write_text("not json\n", encoding="utf-8")
    with pytest.raises(Refused):
        evaluate.guard(tmp_path / "bad")


DECLARED = [
    {"symbol": "Stats", "kind": "record", "lines": [1, 1]},
    {"symbol": "Stats.mean", "kind": "method", "lines": [4, 5]},
    {"symbol": "median", "kind": "function", "lines": [7, 12]},
    {"symbol": 'test "median"', "kind": "test", "lines": [14, 15]},
]


def test_the_baseline_is_the_declarations_holding_the_first_three_diagnostics():
    assert evaluate.baseline_of_check(DECLARED, [8, 5, 9, 1]) == ["median", "Stats.mean"]
    assert evaluate.baseline_of_check(DECLARED, [2]) == []


def test_for_a_failing_test_the_baseline_is_the_first_function_its_block_calls():
    assert evaluate.baseline_of_test(DECLARED, "median([1.0, 2.0]) == len(xs)") == ["median"]
    assert evaluate.baseline_of_test(DECLARED, "len(xs) == 0") == []


def test_top_1_and_top_3_count_a_location_among_the_declarations_the_fix_changed():
    scored = evaluate.score(["median"], ["Stats.mean", "median"], ["median"])
    assert scored == {"top1": False, "top3": True, "baseline_top1": True, "baseline_top3": True}
    assert evaluate.score(["median"], [], [])["top1"] is False


def test_wilson_s_interval_and_the_minimum():
    low, high = evaluate.wilson(50, 97)
    assert high - low <= 0.2
    low, high = evaluate.wilson(5, 10)
    assert high - low > 0.2
    assert evaluate.MINIMUM == 97


def test_an_unreadable_trace_and_a_file_the_safe_layer_refuses_are_counted_not_fatal(
    tmp_path: Path,
):
    (tmp_path / "broken.json").write_text("{not json", encoding="utf-8")
    (tmp_path / "list.json").write_text("[]", encoding="utf-8")
    renamed = trace("mbpp-2")
    for check in renamed["checks"]:
        check["before"] = check["after"] = {"con.lotml": check["before"]["a.lotml"]}
        check["report"]["diagnostics"] = [
            d | {"file": "con.lotml"} for d in check["report"]["diagnostics"]
        ]
    (tmp_path / "renamed.json").write_text(json.dumps(renamed), encoding="utf-8")
    found, refused = evaluate.trace_failures(tmp_path)
    assert found == []
    assert refused == {"unreadable trace": 2, "a file the safe layer refuses": 1}
