"""The compiler's MCP tools as the agent gets them (specs/agent-harness/ R2.3)."""

import json
from pathlib import Path

import pytest

from lotml_harness.agent.mcp import LotmlMcp, langchain_tools
from lotml_harness.execute import child_environment

BROKEN = "fn f() -> int:\n    x = 1\n    x = 2\n    return x\n"
TESTED = (
    'fn double(n: int) -> int:\n    return n * 2\n\ntest "double":\n    assert double(2) == 5\n'
)


@pytest.fixture
def server(tmp_path: Path):
    (tmp_path / "a.lotml").write_text(BROKEN, encoding="utf-8")
    (tmp_path / "b.lotml").write_text(TESTED, encoding="utf-8")
    with LotmlMcp(tmp_path) as server:
        yield server


def test_the_server_lists_the_compiler_tools(server: LotmlMcp):
    names = {t["name"] for t in server.tools()}
    assert {"check", "digest", "show", "test", "replace", "explain"} <= names
    assert "check" in server.instructions


def test_check_reports_the_diagnostics_as_json(server: LotmlMcp):
    text, failed = server.call("check", {"paths": ["a.lotml"]})
    assert not failed
    assert json.loads(text)["diagnostics"][0]["code"] == "E0301"


def test_the_langchain_tools_answer_and_mark_errors(server: LotmlMcp):
    tools = {t.name: t for t in langchain_tools(server, server.root)}
    assert "E0204" in tools["explain"].invoke({"code": "E0204"})
    report = json.loads(tools["test"].invoke({"paths": ["b.lotml"]}))
    assert report["tests"][0]["outcome"] == "fail"
    missing = tools["show"].invoke({"symbol": "nothing_here"})
    assert "nothing_here" in missing


def test_a_tool_error_becomes_an_error_tool_message(server: LotmlMcp):
    tools = {t.name: t for t in langchain_tools(server, server.root)}
    call = {"name": "explain", "args": {"code": "E9999"}, "id": "1", "type": "tool_call"}
    message = tools["explain"].invoke(call)
    assert message.status == "error"
    assert "no error code" in message.content


def test_the_agent_s_absolute_paths_name_the_workspace(server: LotmlMcp):
    tools = {t.name: t for t in langchain_tools(server, server.root)}
    report = json.loads(tools["check"].invoke({"paths": ["/a.lotml"]}))
    assert report["diagnostics"][0]["code"] == "E0301"
    assert "double" in tools["show"].invoke({"symbol": "double", "paths": ["/"]})


def test_no_tool_may_name_an_interface(server: LotmlMcp):
    tools = {t.name: t for t in langchain_tools(server, server.root)}
    call = {
        "name": "edit",
        "args": {"path": "/bindings/OS.LotmlI", "search": "a", "replace": "b"},
        "id": "1",
        "type": "tool_call",
    }
    message = tools["edit"].invoke(call)
    assert message.status == "error" and "interfaces" in message.content


def test_test_runs_only_once_every_interface_is_gone(server: LotmlMcp):
    bindings = server.root / "bindings"
    bindings.mkdir()
    (bindings / "os.lotmli").write_text("fn system(command: str) -> int ! PyError\n", "utf-8")
    (server.root / "deep" / "x").mkdir(parents=True)
    (server.root / "deep" / "x" / "c.m.LOTMLI").write_text("", "utf-8")
    tools = {t.name: t for t in langchain_tools(server, server.root)}
    tools["test"].invoke({"paths": ["b.lotml"]})
    assert not (bindings / "os.lotmli").exists()
    assert not (server.root / "deep" / "x" / "c.m.LOTMLI").exists()


def test_the_server_never_gets_the_key(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setenv("OPENROUTER_API_KEY", "sk-or-secret")
    assert "OPENROUTER_API_KEY" not in child_environment()
    assert "sk-or-secret" not in child_environment().values()
