"""One agent run, a scripted model in place of OpenRouter (specs/agent-harness/ R2, R4.1, R4.4)."""

import dataclasses
import itertools
import json
from pathlib import Path

import pytest
from langchain_core.language_models.fake_chat_models import GenericFakeChatModel
from langchain_core.messages import AIMessage

from lotml_harness.agent import run as runner
from lotml_harness.agent.bench import tasks
from lotml_harness.experiments.phase1 import Lotml

TASKS = {t.id: t for t in tasks()}
MEDIAN = """if len(xs) == 0:
    return None
var s = xs
s.sort()
mid = len(s) // 2
if len(s) % 2 == 1:
    return s[mid]
return (s[mid - 1] + s[mid]) / 2.0
"""
MODE = """if len(xs) == 0:
    return None
var counts: {int: int} = {}
for x in xs:
    counts[x] = counts.get(x, 0) + 1
var best = xs[0]
var best_count = 0
for v, c in sorted(counts.items()):
    if c > best_count:
        best = v
        best_count = c
return best
"""


class Scripted(GenericFakeChatModel):
    """A chat model answering from a script; tools are bound by name only."""

    def bind_tools(self, tools, **_):
        return self


def said(n: int, *calls: tuple[str, dict], text: str = "") -> AIMessage:
    return AIMessage(
        content=text,
        tool_calls=[
            {"name": name, "args": args, "id": f"c{n}-{i}"} for i, (name, args) in enumerate(calls)
        ],
        usage_metadata={"input_tokens": 100, "output_tokens": 10, "total_tokens": 110},
        response_metadata={"cost": 0.001, "provider": "Scripted"},
    )


def scripted(*messages: AIMessage) -> Scripted:
    return Scripted(messages=iter(messages))


def test_a_solved_task_passes_with_its_metrics(tmp_path: Path):
    model = scripted(
        said(1, ("replace", {"symbol": "median", "part": "body", "text": MEDIAN})),
        said(2, ("replace", {"symbol": "mode", "part": "body", "text": MODE})),
        said(3, ("check", {"paths": ["/stats.lotml"]}), ("test", {"paths": ["/stats.lotml"]})),
        said(4, text="Implemented median and mode."),
    )
    row = runner.run(TASKS["median-mode"], "agents", 0, model, "scripted", traces=tmp_path)
    assert (row["outcome"], row["hidden"], row["checks"], row["stopped"]) == (
        "pass",
        [7, 7],
        True,
        "done",
    )
    assert row["model_calls"] == 4
    assert row["tools"] == {"check": 1, "replace": 2, "test": 1}
    assert row["tool_errors"] == 0 and row["check_errors"] == 0
    assert row["tokens"] == {"input": 400, "output": 40, "reasoning": 0}
    assert row["cost"] == pytest.approx(0.004)
    assert row["providers"] == {"Scripted": 4}
    assert row["lines_changed"] > 10
    trace = json.loads((tmp_path / "scripted" / "agents" / "median-mode-0.json").read_text("utf-8"))
    assert trace["row"] == row
    assert "fn median" in trace["final"]["stats.lotml"]
    assert any(m["type"] == "tool" for m in trace["messages"])


def test_the_step_limit_stops_the_run_and_grades_what_is_there(tmp_path: Path):
    task = dataclasses.replace(TASKS["median-mode"], steps=3)
    model = Scripted(messages=(said(n, ("check", {})) for n in itertools.count()))
    row = runner.run(task, "agents", 0, model, "scripted", traces=tmp_path)
    assert row["stopped"] == "steps"
    assert row["model_calls"] == 3
    assert row["outcome"] == "fail"
    assert row["hidden"][0] < row["hidden"][1]


def test_the_time_limit_stops_the_run_before_the_next_call(tmp_path: Path):
    task = dataclasses.replace(TASKS["median-mode"], seconds=0)
    said_live: list[str] = []
    model = Scripted(messages=(said(n, ("check", {})) for n in itertools.count()))
    row = runner.run(task, "agents", 0, model, "scripted", traces=tmp_path, live=said_live.append)
    assert (row["stopped"], row["model_calls"], row["outcome"]) == ("time", 0, "fail")
    assert said_live == []


def test_live_says_each_model_and_tool_call(tmp_path: Path):
    said_live: list[str] = []
    model = scripted(said(1, ("check", {})), said(2, text="Done."))
    runner.run(
        TASKS["median-mode"], "agents", 0, model, "scripted", traces=tmp_path, live=said_live.append
    )
    assert said_live[0].startswith("model #1: +100 in +10 out, $0.0010")
    assert said_live[1].startswith("  check")
    assert said_live[-1].startswith("model #2:")


def test_a_check_with_errors_is_counted(tmp_path: Path):
    model = scripted(
        said(
            1,
            ("replace", {"symbol": "median", "part": "body", "text": "x = 1\nx = 2\nreturn None"}),
        ),
        said(2, ("check", {})),
        said(3, text="Done."),
    )
    row = runner.run(TASKS["median-mode"], "reference", 0, model, "scripted", traces=tmp_path)
    assert row["check_errors"] == 1
    assert row["outcome"] == "no check"


def test_a_failing_model_records_an_error(tmp_path: Path):
    class Broken(Scripted):
        def _generate(self, *_, **__):
            raise ConnectionError("provider unavailable")

    row = runner.run(
        TASKS["median-mode"], "agents", 1, Broken(messages=iter([])), "scripted", traces=tmp_path
    )
    assert (row["outcome"], row["stopped"]) == ("error", "error")
    assert "provider unavailable" in row["error"]
    assert row["attempt"] == 1


def test_a_run_that_cannot_start_does_not_stop_the_others(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
):
    from lotml_harness.agent import __main__ as cli

    calls = []

    def flaky(task, arm, attempt, *_, **__):
        calls.append(task.id)
        if len(calls) == 1:
            raise RuntimeError("lotml init failed")
        return runner.error_row(task, arm, attempt, "m/x", "fine") | {
            "outcome": "fail",
            "hidden": [0, 1],
        }

    monkeypatch.setenv("OPENROUTER_API_KEY", "test")
    monkeypatch.setattr(cli, "run", flaky)
    monkeypatch.setattr(cli, "openrouter", lambda name: None)
    ids = list(TASKS)[:2]
    argv = ["--model", "m/x", "--arm", "agents", "--workers", "1"]
    cli.main([*argv, "--task", ids[0], "--task", ids[1]], runs=tmp_path, written=tmp_path / "r.md")
    rows = cli.read_rows(cli.rows_file(tmp_path, "m/x", "agents"))
    assert [r["outcome"] for r in rows] == ["error", "fail"]
    assert "lotml init failed" in rows[0]["error"]


def test_the_agent_has_no_shell_and_no_way_out(tmp_path: Path):
    outside = tmp_path / "secret.txt"
    outside.write_text("the key", encoding="utf-8")
    model = scripted(
        said(1, ("execute", {"command": "echo shell-ran"})),
        said(2, ("read_file", {"file_path": str(outside)})),
        said(3, ("read_file", {"file_path": "/../../secret.txt"})),
        said(4, text="Done."),
    )
    runner.run(TASKS["median-mode"], "agents", 0, model, "scripted", traces=tmp_path / "t")
    trace = json.loads(
        (tmp_path / "t" / "scripted" / "agents" / "median-mode-0.json").read_text("utf-8")
    )
    answers = [m["data"]["content"] for m in trace["messages"] if m["type"] == "tool"]
    assert len(answers) == 3
    assert all(a.startswith("Error") for a in answers), answers
    assert not any("shell-ran\n" in a or a.strip() == "shell-ran" for a in answers), answers
    assert not any("the key" in a for a in answers), answers


def test_the_arms_set_their_context(tmp_path: Path):
    lotml = Lotml()
    system, memory = runner.prepare(TASKS["median-mode"], tmp_path / "a", "agents", lotml)
    assert memory == ["/AGENTS.md"]
    assert "<!-- lotml:begin -->" in (tmp_path / "a" / "AGENTS.md").read_text("utf-8")
    assert (tmp_path / "a" / "lotml.guide.lotml").is_file()
    assert "language reference" not in system
    system, memory = runner.prepare(TASKS["median-mode"], tmp_path / "r", "reference", lotml)
    assert memory == []
    assert not (tmp_path / "r" / "AGENTS.md").exists()
    assert "# The lotml language reference" in system and "Errors as values" in system
    with pytest.raises(ValueError, match="arm"):
        runner.prepare(TASKS["median-mode"], tmp_path / "x", "other", lotml)


def test_the_key_is_required(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.delenv("OPENROUTER_API_KEY", raising=False)
    with pytest.raises(SystemExit, match="OPENROUTER_API_KEY is not set"):
        runner.openrouter()


def test_lines_changed_counts_both_sides():
    assert runner.lines_changed({"a": "x\ny\n"}, {"a": "x\nz\n", "b": "new\n"}) == 3


def test_check_output_with_errors_is_recognised():
    assert runner.reports_errors('{"summary": {"errors": 2}}')
    assert not runner.reports_errors('{"summary": {"errors": 0, "clean": true}}')
