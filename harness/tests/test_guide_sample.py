"""Answers sampled from the guide, judged by the compiler and counted
(specs/training-pipeline/ R3.6, R5.1)."""

import json

import pytest

from lotml_harness.guide import sample
from lotml_harness.guide.reward import Judged

STATE = {"task": None, "path": "solution.lot", "text": "x", "diagnostics": [], "failing": None}


def record(symbol: str = "count", problem: str = "humaneval/0") -> dict:
    target = {
        "locations": [{"path": "solution.lot", "symbol": symbol, "lines": [1, 2]}],
        "kind": "body",
        "edit": None,
    }
    return {
        "messages": [
            {"role": "user", "content": "u"},
            {"role": "assistant", "content": json.dumps(target)},
        ],
        "state": STATE,
        "meta": {"problem": problem, "split": "train"},
    }


def test_a_draw_keeps_order_and_is_the_same_for_a_seed():
    records = [record(problem=f"humaneval/{i}") for i in range(10)]
    first = sample.drawn(records, 4, seed=1)
    assert len(first) == 4 and first == sample.drawn(records, 4, seed=1)
    order = [int(r["meta"]["problem"].split("/")[1]) for r in first]
    assert order == sorted(order)
    assert sample.drawn(records, None, 0) == records


def test_answers_are_judged_scored_and_averaged(monkeypatch: pytest.MonkeyPatch):
    verdicts = {
        "right": Judged(True, ["count"], "passes"),
        "located": Judged(True, ["count"], "edit-fails-check"),
        "broken": None,
    }
    monkeypatch.setattr(sample.reward, "judge", lambda text, state: verdicts[text])
    [row] = sample.judged([record()], [["right", "located", "broken"]], workers=1)
    assert row["truth"] == ["count"]
    assert [a["reward"] for a in row["answers"]] == [1.0, 0.5, 0.0]
    assert [a["first"] for a in row["answers"]] == [True, True, False]
    assert [a["judged"] for a in row["answers"]] == [True, True, False]
    assert row["mean"] == pytest.approx(0.5)
    assert [sample.passed(a) for a in row["answers"]] == [True, False, False]


def test_pass_at_k_is_the_unbiased_estimate():
    assert sample.pass_at(8, 0, 1) == 0.0
    assert sample.pass_at(8, 8, 4) == 1.0
    assert sample.pass_at(8, 1, 1) == pytest.approx(1 / 8)
    assert sample.pass_at(8, 1, 4) == pytest.approx(1 - 35 / 70)
    assert sample.pass_at(8, 5, 4) == 1.0


def test_pass_rates_count_locations_and_whole_answers():
    right = {"first": True, "edit": "passes"}
    located = {"first": True, "edit": "none"}
    wrong = {"first": False, "edit": "none"}
    rows = [
        {"answers": [right, wrong, wrong, wrong]},
        {"answers": [located, located, wrong, wrong]},
    ]
    rates = sample.pass_rates(rows, ks=(1, 4, 8))
    assert rates["location@1"] == pytest.approx((1 / 4 + 2 / 4) / 2)
    assert rates["answer@1"] == pytest.approx((1 / 4 + 0) / 2)
    assert rates["location@4"] == 1.0 and rates["answer@4"] == pytest.approx(0.5)
    assert "location@8" not in rates
