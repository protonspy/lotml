"""pass@k and the Wilson interval, against values worked by hand, and the report and resumption
built on them (specs/agent-harness/ R4.2, R4.3)."""

import json
from pathlib import Path

import pytest

from lotml_harness.agent.__main__ import main, pending, rows_file
from lotml_harness.agent.bench import tasks
from lotml_harness.agent.report import markdown, pass_at_k, summarize, wilson


def row(task: str, attempt: int, outcome: str, arm: str = "agents", **extra) -> dict:
    return {
        "task": task,
        "kind": "fix",
        "model": "m/x",
        "arm": arm,
        "attempt": attempt,
        "outcome": outcome,
        "hidden": [3, 3] if outcome == "pass" else [1, 3],
        "checks": outcome != "no check",
        "stopped": "done",
        "model_calls": 4,
        "tools": {"check": 2},
        "tool_errors": 0,
        "check_errors": 1,
        "tokens": {"input": 1000, "output": 100, "reasoning": 0},
        "cost": 0.002,
        "seconds": 10.0,
        "lines_changed": 5,
        "error": "boom" if outcome == "error" else None,
    } | extra


def test_pass_at_1_is_the_pass_rate():
    assert pass_at_k(5, 2, 1) == pytest.approx(0.4)
    assert pass_at_k(4, 0, 1) == 0.0
    assert pass_at_k(4, 4, 1) == 1.0


def test_pass_at_k_is_one_minus_the_chance_that_every_draw_fails():
    # 1 - C(3, 2) / C(5, 2) = 1 - 3/10
    assert pass_at_k(5, 2, 2) == pytest.approx(0.7)
    # 1 - C(7, 3) / C(10, 3) = 1 - 35/120
    assert pass_at_k(10, 3, 3) == pytest.approx(85 / 120)


def test_pass_at_k_is_certain_when_fewer_runs_fail_than_are_drawn():
    assert pass_at_k(5, 4, 2) == 1.0
    assert pass_at_k(3, 1, 3) == 1.0


def test_pass_at_k_needs_k_runs():
    with pytest.raises(ValueError):
        pass_at_k(2, 1, 3)
    with pytest.raises(ValueError):
        pass_at_k(3, 4, 1)


def test_wilson_matches_the_worked_values():
    low, high = wilson(8, 10)
    assert (low, high) == (pytest.approx(0.4902, abs=1e-4), pytest.approx(0.9433, abs=1e-4))
    low, high = wilson(0, 10)
    assert low == pytest.approx(0.0, abs=1e-12)
    assert high == pytest.approx(0.2775, abs=1e-4)
    low, high = wilson(10, 10)
    assert (low, high) == (pytest.approx(0.7225, abs=1e-4), pytest.approx(1.0, abs=1e-12))


def test_wilson_of_no_runs_says_nothing():
    assert wilson(0, 0) == (0.0, 1.0)


def test_the_summary_averages_pass_at_k_over_tasks_and_leaves_errors_out():
    rows = [
        row("a", 0, "pass"),
        row("a", 1, "fail"),
        row("b", 0, "fail"),
        row("b", 1, "no check"),
        row("b", 2, "error"),
        row("a", 0, "pass", arm="reference"),
    ]
    s = summarize(rows)[("m/x", "agents")]
    assert (s["runs"], s["errors"], s["tasks"], s["passed"]) == (4, 1, 2, 1)
    assert s["pass_at"] == {1: pytest.approx(0.25), 2: pytest.approx(0.5)}
    assert s["wilson"] == wilson(1, 4)
    assert s["hidden"] == pytest.approx((1 + 1 / 3 + 1 / 3 + 1 / 3) / 4)
    assert s["checks"] == 3
    assert s["per_run"]["cost (USD)"] == pytest.approx(0.002)
    assert summarize(rows)[("m/x", "reference")]["pass_at"] == {1: 1.0}


def test_a_rerun_replaces_the_row_it_repeats():
    s = summarize([row("a", 0, "error"), row("a", 0, "pass")])[("m/x", "agents")]
    assert (s["runs"], s["errors"], s["passed"]) == (1, 0, 1)


def test_the_report_has_the_arms_and_the_tasks():
    text = markdown([row("a", 0, "pass"), row("a", 0, "fail", arm="reference")])
    assert "| m/x | agents | 1 | 1 | 1.00 |" in text
    assert "| m/x | reference | 1 | 1 | 0.00 |" in text
    assert "| a | fix | 1/1 | 0/1 |" in text


def test_recorded_runs_are_skipped_and_errors_run_again():
    found = tasks()[:2]
    done = [row(found[0].id, 0, "pass"), row(found[1].id, 0, "error")]
    todo = [(t.id, a) for t, a in pending(found, 2, done)]
    assert todo == [(found[0].id, 1), (found[1].id, 0), (found[1].id, 1)]


def test_the_report_is_written_from_the_rows_alone(tmp_path: Path):
    rows = rows_file(tmp_path, "m/x", "agents")
    rows.write_text(json.dumps(row("a", 0, "pass")) + "\n", encoding="utf-8")
    written = tmp_path / "agent.md"
    main(["--report-only"], runs=tmp_path, written=written)
    assert rows.name == "m__x__agents.jsonl"
    assert "| m/x | agents |" in written.read_text(encoding="utf-8")
