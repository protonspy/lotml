"""Running HumanEval's tasks and reporting them by source (specs/agent-humaneval/ R2.1-R2.5)."""

import dataclasses
from pathlib import Path

import pytest

from lotml_harness.agent import __main__ as agent_main
from lotml_harness.agent import humaneval
from lotml_harness.agent.bench import AgentTask
from lotml_harness.agent.grade import Grade
from lotml_harness.agent.report import arms_compared, by_source, markdown
from lotml_harness.tasks.sources import DigestMismatch


def kept(n: int) -> AgentTask:
    return AgentTask(
        id=f"humaneval-{n}",
        kind="implement",
        graded=("solution.lot",),
        prompt="Write it.",
        workspace_files={"solution.lot": ""},
        hidden_files={"solution.lot": ""},
    )


@pytest.fixture
def ran(monkeypatch: pytest.MonkeyPatch, tmp_path: Path) -> list[str]:
    found: list[str] = []
    report = humaneval.Report(digest="d")
    monkeypatch.setattr(humaneval, "humaneval_tasks", lambda: ([kept(n) for n in range(6)], report))
    monkeypatch.setattr(humaneval, "write_humaneval_report", lambda built: None)
    monkeypatch.setattr(agent_main, "openrouter", lambda model: None)

    def run(task, arm, attempt, model, name, live=None):
        found.append(task.id)
        raise RuntimeError("scripted")

    monkeypatch.setattr(agent_main, "run", run)
    return found


def test_a_seeded_sample_of_humaneval_runs_the_same_tasks_in_every_arm(ran, tmp_path: Path):
    agent_main.main(
        ["--source", "humaneval", "--sample", "3", "--seed", "7"],
        runs=tmp_path,
        written=tmp_path / "agent.md",
    )
    drawn = humaneval.sample([f"humaneval-{n}" for n in range(6)], 3, 7)
    assert sorted(ran) == sorted(drawn * 2), "both arms, the same three"


def test_a_named_humaneval_task_runs_alone(ran, tmp_path: Path):
    agent_main.main(["--task", "humaneval-4", "--arm", "agents"], runs=tmp_path,
                    written=tmp_path / "agent.md")  # fmt: skip
    assert ran == ["humaneval-4"]


def test_a_sample_without_humaneval_is_refused(ran, tmp_path: Path):
    with pytest.raises(SystemExit):
        agent_main.main(["--sample", "3"], runs=tmp_path, written=tmp_path / "agent.md")


def test_when_humaneval_cannot_be_read_nothing_runs(ran, monkeypatch, tmp_path: Path, capsys):
    def unreadable():
        raise DigestMismatch("HumanEval.jsonl.gz: SHA-256 x, pinned y")

    monkeypatch.setattr(humaneval, "humaneval_tasks", unreadable)
    agent_main.main(["--source", "humaneval"], runs=tmp_path, written=tmp_path / "agent.md")
    assert ran == []
    assert "nothing runs" in capsys.readouterr().err


def row(task: str, arm: str, outcome: str, source: str | None, failure=None) -> dict:
    found = {
        "task": task, "kind": "implement", "model": "m", "arm": arm, "attempt": 0,
        "outcome": outcome, "hidden": [1, 1], "checks": True, "stopped": "done",
        "model_calls": 1, "tools": {}, "tool_errors": 0, "check_errors": 0,
        "tokens": {"input": 1, "output": 1, "reasoning": 0}, "cost": 0.0, "providers": {},
        "seconds": 1.0, "lines_changed": 1, "failures": [], "error": None, "failure": failure,
    }  # fmt: skip
    return found | ({"source": source} if source else {})


def test_rows_report_pass_at_1_by_source_a_row_without_one_being_the_benchmark_s():
    rows = [
        row("median-mode", "agents", "pass", None),
        row("humaneval-1", "agents", "fail", "humaneval-original", "signature"),
        row("humaneval-2", "agents", "pass", "humaneval-original"),
    ]
    assert by_source(rows) == {
        ("m", "agents", "bench"): (1, 1),
        ("m", "agents", "humaneval-original"): (2, 1),
    }
    text = markdown(rows)
    assert "| m | agents | humaneval-original | 2 | 0.50 |" in text
    assert "signature 1" in text


def test_the_arms_are_compared_on_first_attempts_by_mcnemar():
    rows = [
        row("humaneval-1", "agents", "pass", "humaneval-original"),
        row("humaneval-1", "reference", "fail", "humaneval-original", "behaviour"),
        row("humaneval-2", "agents", "pass", "humaneval-original"),
        row("humaneval-2", "reference", "pass", "humaneval-original"),
        row("humaneval-3", "agents", "pass", "humaneval-original"),
    ]
    tasks, only_agents, only_reference, p = arms_compared(rows)["m"]
    assert (tasks, only_agents, only_reference) == (2, 1, 0)
    assert p == pytest.approx(1.0)
    assert "## The arms compared" in markdown(rows)


def test_a_failure_whose_files_check_alone_but_not_with_the_hidden_blocks_is_a_signature():
    refused = Grade(True, 0, 2, ["solution.lot: the tests did not run"])
    assert refused.failure == "signature"
    assert Grade(True, 1, 2, ["solution.lot: hidden: 1: fail"]).failure == "behaviour"
    assert Grade(False, 0, 2, ["solution.lot: the tests did not run"]).failure == "behaviour"
    assert Grade(True, 2, 2).failure is None


def test_a_named_mbpp_task_runs_from_mbpp(ran, monkeypatch, tmp_path: Path):
    report = humaneval.Report(digest="d")
    mbpp = [dataclasses.replace(kept(n), id=f"mbpp-{n}") for n in (11, 12)]
    monkeypatch.setattr(humaneval, "mbpp_tasks", lambda: (mbpp, report))
    monkeypatch.setattr(humaneval, "write_mbpp_report", lambda built: None)
    agent_main.main(["--task", "mbpp-12", "--arm", "agents"], runs=tmp_path,
                    written=tmp_path / "agent.md")  # fmt: skip
    assert ran == ["mbpp-12"]
