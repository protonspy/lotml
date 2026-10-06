"""The export: permitted runs as trajectories and repairs, by split, with the manifest
(specs/trace-dataset/ R1.2, R1.7, R3.1, R3.5, R3.8, R4.5, R4.6)."""

import json
from pathlib import Path

from lotml_harness.agent import dataset, registry

REGISTRY = """\
[sources.bench]
licence = "MIT"
training = "permitted"
evidence = "LICENSE"
checked = 2026-10-06
notice = "bench notice"

[sources.humaneval-original]
licence = "MIT"
training = "permitted"
evidence = "LICENSE"
checked = 2026-10-06
notice = "humaneval notice"

[models."m/x"]
terms = "https://example.com/terms"
training = "permitted"
evidence = "https://example.com/terms"
checked = 2026-10-06
notice = "model notice"

[models."m/x".providers."P"]
training = "permitted"
evidence = "https://example.com/p"
checked = 2026-10-06
notice = "provider notice"
"""
BROKEN = "fn f() -> int:\n    return x\n"
FIXED = "fn f() -> int:\n    return 1\n"


def check(text: str, errors: int) -> dict:
    diagnostics = [{"file": "a.lotml", "severity": "error", "code": "E0201"}] * errors
    files = {"a.lotml": text}
    return {"paths": [], "before": files, "after": files, "status": "success", "truncated": False,
            "report": {"diagnostics": diagnostics, "summary": {"errors": errors}}}  # fmt: skip


def write(traces: Path, task: str, model: str = "m/x", outcome: str = "pass", final=FIXED) -> None:
    row = {
        "task": task,
        "source": dataset.source_of(task),
        "model": model,
        "arm": "agents",
        "attempt": 0,
        "outcome": outcome,
        "providers": {"P": 3},
        "compiler": "lotml 0.1.0",
    }
    trace = {
        "row": row,
        "system": "You are a coding agent.",
        "tools": [{"type": "function", "function": {"name": "check"}}],
        "messages": [
            {"type": "human", "data": {"content": "Fix f."}},
            {"type": "ai", "data": {"content": "", "tool_calls": [
                {"id": "c1", "name": "check", "args": {"paths": ["/a.lotml"]}}]}},
            {"type": "tool", "data": {"content": "{}", "tool_call_id": "c1"}},
            {"type": "ai", "data": {"content": "Done.", "tool_calls": []}},
        ],
        "final": {"a.lotml": final},
        "checks": [check(BROKEN, 1), check(FIXED, 0)],
        "tests": [],
    }  # fmt: skip
    path = traces / model.replace("/", "__") / "agents" / f"{task}-0.json"
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(trace), encoding="utf-8")


def test_a_permitted_passing_run_gives_its_trajectory_and_repair_by_split(tmp_path: Path):
    traces, out = tmp_path / "traces", tmp_path / "out"
    write(traces, "median-mode")
    write(traces, "humaneval-4")
    write(traces, "stock-take", model="other/y")
    write(traces, "shapes-triangle", final=FIXED + '\ntest "h":\n    assert hidden(1) == 2\n')
    (tmp_path / "licences.toml").write_text(REGISTRY, encoding="utf-8")
    found = registry.read(tmp_path / "licences.toml")
    hidden = {"shapes-triangle": {"assert hidden(1) == 2"}}
    manifest = dataset.export(traces, found, lambda task: hidden.get(task, set()), out)
    [trajectory] = [json.loads(line) for line in (out / "train" / "trajectories.jsonl").open()]
    assert [m["role"] for m in trajectory["messages"]] == [
        "system",
        "user",
        "assistant",
        "tool",
        "assistant",
    ]
    assert trajectory["messages"][2]["tool_calls"][0]["function"]["name"] == "check"
    assert trajectory["files"] == {"a.lotml": FIXED}
    assert (
        trajectory["meta"] | {} == trajectory["meta"]
        and trajectory["meta"]["problem"] == "bench/median-mode"
    )
    repairs = [json.loads(line) for line in (out / "train" / "repairs.jsonl").open()]
    assert {r["meta"]["problem"] for r in repairs} == {"bench/median-mode", "bench/shapes-triangle"}
    assert all(r["meta"]["split"] == "train" for r in repairs)
    assert (out / "validation" / "repairs.jsonl").read_text(encoding="utf-8") == ""
    assert manifest.left_out == {"held out": 1, "model other/y has no entry": 1}
    assert manifest.dropped == {"a hidden test": 1}
    notice = (out / "NOTICE").read_text(encoding="utf-8")
    assert (
        "bench notice" in notice
        and "provider notice" in notice
        and "humaneval notice" not in notice
    )
    text = dataset.markdown(manifest, "2026-10-06")
    assert "| bench | train | 2 | 1 | 2 |" in text
    assert "| held out | 1 |" in text
    assert "| a hidden test | 1 |" in text
    assert "| provider | m/x via P |" in text


def test_a_failing_run_gives_no_trajectory_but_its_repairs():
    trace = {"row": {"task": "median-mode", "outcome": "fail"}, "messages": [], "checks": []}
    assert dataset.trajectory(trace) is None


def test_an_unreadable_trace_and_a_task_with_no_tests_are_left_out_not_fatal(tmp_path: Path):
    traces, out = tmp_path / "traces", tmp_path / "out"
    write(traces, "median-mode")
    write(traces, "stock-take")
    (traces / "m__x" / "agents" / "broken-0.json").write_text("{not json", encoding="utf-8")
    (tmp_path / "licences.toml").write_text(REGISTRY, encoding="utf-8")
    found = registry.read(tmp_path / "licences.toml")

    def hidden_for(task: str) -> set[str]:
        if task == "stock-take":
            raise LookupError(task)
        return set()

    manifest = dataset.export(traces, found, hidden_for, out)
    assert manifest.left_out == {"unreadable trace": 1, "unknown task": 1}
    assert manifest.runs == 3


def test_an_exported_record_names_no_home_directory(tmp_path: Path):
    traces, out = tmp_path / "traces", tmp_path / "out"
    write(traces, "median-mode", final=FIXED + f"# {Path.home()}\n")
    (tmp_path / "licences.toml").write_text(REGISTRY, encoding="utf-8")
    dataset.export(traces, registry.read(tmp_path / "licences.toml"), lambda task: set(), out)
    text = (out / "train" / "trajectories.jsonl").read_text(encoding="utf-8")
    assert str(Path.home()).replace("\\", "\\\\") not in text and "# ~" in text


def test_a_source_is_read_only_from_the_spellings_the_harness_writes():
    assert dataset.source_of("humaneval-12") == "humaneval-original"
    assert dataset.source_of("mbpp-7") == "mbpp-original"
    assert dataset.source_of("median-mode") == "bench"
    assert dataset.source_of("HumanEval_12_x") == "unknown"
    assert dataset.source_of("humaneval/12") == "unknown"


def test_a_manifest_cell_cannot_split_its_row():
    assert dataset.cell("provider a|b\nc") == "provider a\\|b c"
