"""One agent run, a scripted model in place of OpenRouter (specs/agent-harness/ R2, R4.1, R4.4)."""

import dataclasses
import itertools
import json
from pathlib import Path

import pytest
from langchain_core.language_models.fake_chat_models import GenericFakeChatModel
from langchain_core.messages import AIMessage
from langchain_core.utils.function_calling import convert_to_openai_tool

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
    """A chat model answering from a script; its tools are bound as a real model's are, so their
    schemas reach its invocation parameters."""

    def bind_tools(self, tools, **_):
        return self.bind(tools=[convert_to_openai_tool(t) for t in tools])


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
        said(3, ("check", {"paths": ["/stats.lot"]}), ("test", {"paths": ["/stats.lot"]})),
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
    assert row["source"] == "bench"
    assert row["tools"] == {"check": 1, "replace": 2, "test": 1}
    assert row["tool_errors"] == 0 and row["check_errors"] == 0
    assert row["tokens"] == {"input": 400, "output": 40, "reasoning": 0}
    assert row["cost"] == pytest.approx(0.004)
    assert row["providers"] == {"Scripted": 4}
    assert row["lines_changed"] > 10
    trace = json.loads((tmp_path / "scripted" / "agents" / "median-mode-0.json").read_text("utf-8"))
    assert trace["row"] == row
    assert "fn median" in trace["final"]["stats.lot"]
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


def test_the_agent_cannot_write_an_interface(tmp_path: Path):
    model = scripted(
        said(
            1, ("write_file", {"file_path": "/bindings/os.lotmli", "content": "fn getcwd() -> str"})
        ),
        said(2, ("write_file", {"file_path": "/Deep/OS.LOTMLI", "content": "x"})),
        said(3, text="Done."),
    )
    runner.run(TASKS["median-mode"], "agents", 0, model, "scripted", traces=tmp_path)
    trace = json.loads((tmp_path / "scripted" / "agents" / "median-mode-0.json").read_text("utf-8"))
    answers = [m["data"] for m in trace["messages"] if m["type"] == "tool"]
    assert len(answers) == 2
    assert all(a["status"] == "error" or "denied" in a["content"].lower() for a in answers), answers
    assert not any(name.endswith((".lotmli", ".LOTMLI")) for name in trace["final"])


def test_the_arms_set_their_context(tmp_path: Path):
    lotml = Lotml()
    system, memory = runner.prepare(TASKS["median-mode"], tmp_path / "a", "agents", lotml)
    assert memory == ["/AGENTS.md"]
    assert "<!-- lotml:begin -->" in (tmp_path / "a" / "AGENTS.md").read_text("utf-8")
    assert (tmp_path / "a" / "lotml.guide.lot").is_file()
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


def test_the_model_waits_minutes_not_milliseconds(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setenv("OPENROUTER_API_KEY", "test")
    model = runner.openrouter()
    assert model.request_timeout == runner.REQUEST_TIMEOUT * 1000
    assert model.model_name == runner.MODEL


def test_lines_changed_counts_both_sides():
    assert runner.lines_changed({"a": "x\ny\n"}, {"a": "x\nz\n", "b": "new\n"}) == 3


def test_check_output_with_errors_is_recognised():
    assert runner.reports_errors('{"summary": {"errors": 2}}')
    assert not runner.reports_errors('{"summary": {"errors": 0, "clean": true}}')


def test_the_trace_keeps_the_files_around_every_check_and_test_and_the_context(tmp_path: Path):
    broken = "x = 1\nx = 2\nreturn None"
    model = scripted(
        said(1, ("replace", {"symbol": "median", "part": "body", "text": broken})),
        said(2, ("check", {"paths": ["/stats.lot"]})),
        said(3, ("replace", {"symbol": "median", "part": "body", "text": MEDIAN})),
        said(4, ("check", {"paths": ["/stats.lot"]})),
        said(5, ("test", {"paths": ["/stats.lot"]})),
        said(6, text="Done."),
    )
    row = runner.run(TASKS["median-mode"], "agents", 0, model, "scripted", traces=tmp_path)
    trace = json.loads((tmp_path / "scripted" / "agents" / "median-mode-0.json").read_text("utf-8"))
    first, second = trace["checks"]
    assert first["paths"] == ["stats.lot"], "the file tools' leading / is stripped"
    assert first["status"] == "success" and not first["truncated"]
    assert first["before"] == first["after"], "check changes no file"
    assert "x = 2" in first["before"]["stats.lot"]
    assert first["report"]["summary"]["errors"] > 0
    assert second["report"]["summary"]["errors"] == 0
    assert "mid = len(s) // 2" in second["before"]["stats.lot"]
    [test] = trace["tests"]
    assert test["paths"] == ["stats.lot"]
    assert test["status"] == "success" and test["report"]["tests"]
    assert trace["compiler"].startswith("lotml ") and row["compiler"] == trace["compiler"]
    assert row["python"]["version"].startswith("3."), "the row records the CPython it graded on"
    assert row["python"]["path"] and "uv" in row["python"]
    assert str(Path.home()) not in row["python"]["path"], "the committed row names no home"
    assert runner.SYSTEM in trace["system"]
    assert {t["function"]["name"] for t in trace["tools"]} >= {"check", "test", "replace"}


def test_a_snapshot_skips_links_and_marks_a_cut(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    (tmp_path / "a.lotml").write_text("fn f() -> int:\n    return 1\n", encoding="utf-8")
    (tmp_path / "big.lotml").write_text("x" * 100, encoding="utf-8")
    monkeypatch.setattr(runner, "SNAPSHOT_FILE", 50)
    files, cut = runner.bounded_snapshot(tmp_path)
    assert list(files) == ["a.lotml"]
    assert cut
    try:
        (tmp_path / "link.lotml").symlink_to(tmp_path / "a.lotml")
    except OSError:
        return
    files, _ = runner.bounded_snapshot(tmp_path)
    assert "link.lotml" not in files


def test_a_snapshot_takes_lot_and_lotml_files_and_nothing_else(tmp_path: Path):
    for name in ("a.lot", "b.lotml", "sub/c.lot", "d.lotmli", "e.py"):
        (tmp_path / name).parent.mkdir(exist_ok=True)
        (tmp_path / name).write_text("x\n", encoding="utf-8")
    (tmp_path / "dir.lot").mkdir()
    files, cut = runner.bounded_snapshot(tmp_path)
    assert list(files) == ["a.lot", "b.lotml", "sub/c.lot"] and not cut
    assert list(runner.snapshot(tmp_path)) == ["a.lot", "b.lotml", "sub/c.lot"]


def test_a_test_report_names_no_home_directory_or_user():
    home, user = str(Path.home()), Path.home().name
    text = json.dumps({"message": f"cannot load {home}/x.py or D:/work/{user}/y.py"})
    shown = runner.anonymised(text)
    assert home not in shown and user not in shown
    assert "~" in shown and "D:/work/<user>/y.py" in shown


def test_a_word_holding_the_user_name_is_left_alone(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setattr(Path, "home", lambda: Path("/home/test"))
    shown = runner.anonymised("the contest in /srv/test/a.py and /srv/tester/b.py")
    assert shown == "the contest in /srv/<user>/a.py and /srv/tester/b.py"


def test_a_response_naming_no_provider_is_counted_under_one_the_registry_does_not_hold():
    from langchain_core.outputs import ChatGeneration, LLMResult

    meter = runner.Meter()
    named, unnamed = said(0), said(1)
    unnamed.response_metadata = {"cost": 0.001}
    generations = [[ChatGeneration(message=named)], [ChatGeneration(message=unnamed)]]
    meter.on_llm_end(LLMResult(generations=generations))
    assert meter.providers == {"Scripted": 1, runner.UNNAMED: 1}
