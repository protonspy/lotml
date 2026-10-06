"""Guidance records from repairs (specs/guide-records/ R2.1-R2.7)."""

import json
from pathlib import Path

import pytest

from lotml_harness.guide import records
from lotml_harness.guide.records import Budget, HeldOut, build, target

AFTER = "fn add(a: int, b: int) -> int:\n    return a + b\n"
BEFORE = "fn add(a: int, b: int) -> int:\n    return a + bs\n"
DIAGNOSTIC = {
    "code": "E0201",
    "severity": "error",
    "message": "`bs` is not defined",
    "location": {"line": 2, "column": 16},
}


def repair(problem: str = "humaneval/0", bucket: str = "train") -> dict:
    return {
        "prompt": "Write `add`.",
        "path": "solution.lotml",
        "before": BEFORE,
        "after": AFTER,
        "diagnostics": [DIAGNOSTIC],
        "failing": None,
        "changed": [2],
        "meta": {
            "task": "humaneval-0",
            "source": "humaneval-original",
            "model": None,
            "compiler": "lotml 0.1.0",
            "problem": problem,
            "split": bucket,
            "origin": "seeded",
        },
    }


ROOMY = Budget(str.split, 10_000, 512)


def test_the_target_is_the_changed_declarations_the_kind_and_the_edit():
    diff = {
        "declarations": [
            {"symbol": f"f{i}", "kind": "function", "lines": [i, i]} for i in range(5)
        ],
        "edit": {
            "tool": "replace",
            "kind": "body",
            "arguments": {"symbol": "f0", "path": "a.lotml"},
        },
    }
    found = target(diff, "a.lotml")
    assert [loc["symbol"] for loc in found["locations"]] == ["f0", "f1", "f2"]
    assert found["locations"][0] == {"path": "a.lotml", "symbol": "f0", "lines": [0, 0]}
    assert (found["kind"], found["edit"]["tool"]) == ("body", "replace")
    assert "kind" not in found["edit"]
    several = target({"declarations": diff["declarations"][:1], "edit": None}, "a.lotml")
    assert (several["kind"], several["edit"]) == ("several", None)


def test_a_repair_becomes_a_chat_example_with_and_without_its_task():
    made, left_out = build(repair(), ROOMY)
    assert left_out == []
    assert [r["meta"]["task"] for r in made] == [True, False]
    with_task, without = made
    roles = [m["role"] for m in with_task["messages"]]
    assert roles == ["system", "user", "assistant"]
    assert with_task["messages"][1]["content"].startswith(
        "Task: Write `add`.\n\nFile solution.lotml:"
    )
    assert not without["messages"][1]["content"].startswith("Task:")
    assert "error E0201 at 2:16" in with_task["messages"][1]["content"]
    answer = json.loads(with_task["messages"][2]["content"])
    assert answer["locations"] == [{"path": "solution.lotml", "symbol": "add", "lines": [1, 2]}]
    assert answer["kind"] == "body"
    assert answer["edit"]["arguments"]["path"] == "solution.lotml"
    assert with_task["meta"] == {
        "problem": "humaneval/0",
        "split": "train",
        "origin": "seeded",
        "source": "humaneval-original",
        "model": None,
        "compiler": "lotml 0.1.0",
        "renderer": 1,
        "task": True,
        "kind": "body",
    }


def test_the_split_is_read_again_and_a_held_out_repair_stops_the_build():
    with pytest.raises(HeldOut):
        build(repair("humaneval/4", "train"), ROOMY)
    with pytest.raises(HeldOut):
        build(repair("mbpp/2", "validation"), ROOMY)


def test_a_record_over_the_context_is_left_out_and_counted():
    made, left_out = build(repair(), Budget(str.split, 600, 512))
    assert made == []
    assert left_out == ["over the context", "over the context"]


def test_records_are_written_train_and_validation_apart(tmp_path: Path):
    train = {"meta": {"split": "train"}}
    validation = {"meta": {"split": "validation"}}
    out = records.write([train, validation], "2026-10-06", tmp_path)
    assert json.loads((out / "train.jsonl").read_text(encoding="utf-8")) == train
    assert json.loads((out / "validation.jsonl").read_text(encoding="utf-8")) == validation


def test_the_report_counts_records_by_origin_split_kind_and_source_and_what_was_left_out():
    tally = records.Tally()
    tally.records[("seeded", "train", "body", "bench", True)] = 3
    tally.records[("seeded", "validation", "arm", "humaneval-original", False)] = 2
    tally.left_out["over the context"] = 1
    text = records.markdown(tally, ROOMY, "Qwen/x", ["harness/cache/guide/seeded/d"], "2026-10-06")
    for row in ("| seeded | 5 |", "| train | 3 |", "| arm | 2 |", "| bench | 3 |"):
        assert row in text
    assert "5 records, 3 of them with the task." in text
    assert "| over the context | 1 |" in text
