"""The validation split asked through the guide tool, its metrics and the calibrated threshold
(specs/training-pipeline/ R3.4, R5.1)."""

import http.server
import json
import threading
from pathlib import Path
from typing import ClassVar

import pytest

from lotml_harness.guide import stages, validate

BROKEN = "fn count() -> int:\n    n = 0\n    n += 1\n    return n\n"


def answer(body: str | None) -> dict:
    edit = None
    if body is not None:
        edit = {
            "tool": "replace",
            "arguments": {
                "path": "solution.lotml",
                "symbol": "count",
                "part": "body",
                "text": body,
            },
        }
    locations = [{"path": "solution.lotml", "symbol": "count", "lines": [1, 4]}]
    return {"locations": locations, "kind": "body", "edit": edit}


class Guide(http.server.BaseHTTPRequestHandler):
    """A guide server answering every request with `content`, one token at probability 0.99."""

    content: ClassVar[str] = ""

    def do_POST(self):
        self.rfile.read(int(self.headers["Content-Length"]))
        token = [{"token": self.content, "logprob": -0.01}]
        body = {"choices": [{"message": {"content": self.content}, "logprobs": {"content": token}}]}
        data = json.dumps(body).encode()
        self.send_response(200)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *_):
        pass


@pytest.fixture
def config(tmp_path: Path):
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Guide)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    (tmp_path / "records").mkdir()
    url = f"http://127.0.0.1:{server.server_port}"
    yield validate.configuration(url, tmp_path / "records", tmp_path / "guide.toml")
    server.shutdown()


def record() -> dict:
    target = json.dumps(answer("    var n = 0\n    n += 1\n    return n"))
    state = {
        "task": None,
        "path": "solution.lotml",
        "text": BROKEN,
        "diagnostics": [],
        "failing": None,
    }
    return {"messages": [{"role": "assistant", "content": target}], "state": state}


def test_an_edit_that_fixes_the_file_is_shown_and_its_location_counted(config):
    Guide.content = json.dumps(answer("    var n = 0\n    n += 1\n    return n"))
    row = validate.asked(record(), config)
    assert (row["silent"], row["top1"], row["top3"], row["edit"]) == (False, True, True, True)
    assert row["confidence"] > 0.98


def test_a_copied_body_is_withheld_and_an_answer_outside_the_schema_is_silent(config):
    Guide.content = json.dumps(answer("    n = 0\n    n += 1\n    return n"))
    copied = validate.asked(record(), config)
    assert (copied["top1"], copied["edit"]) == (True, False)
    Guide.content = '{"locations": []}'
    invalid = validate.asked(record(), config)
    assert (invalid["silent"], invalid["reason"], invalid["top1"]) == (
        True,
        "invalid-answer",
        False,
    )


def row(
    top1: bool, confidence: float | None, edit: bool = False, reason: str | None = None
) -> dict:
    return {
        "silent": reason is not None,
        "reason": reason,
        "top1": top1,
        "top3": top1,
        "edit": edit,
        "confidence": confidence,
    }


def test_metrics_are_shares_of_every_record_asked():
    rows = [
        row(True, 0.9, edit=True),
        row(False, 0.5),
        row(False, None, reason="invalid-answer"),
        row(True, 0.8),
    ]
    found = validate.metrics(rows)
    assert found == {
        "records": 4, "within_schema": 0.75, "top1": 0.5, "top3": 0.5, "edits_pass": 0.25,
        "silent": {"invalid-answer": 1},
    }  # fmt: skip


def test_the_threshold_keeps_the_answers_at_or_above_it_at_the_target_precision():
    rows = [row(True, 0.95), row(True, 0.9), row(False, 0.7), row(True, 0.6), row(False, 0.2)]
    threshold, precision, shown = validate.calibrated(rows, target=0.9)
    assert (threshold, precision, shown) == (0.9, 1.0, 0.4)


class Store:
    """A repository listing `files`, serving each `<stage>.json` report from `reports`."""

    def __init__(self, files: list[str], reports: dict[str, dict] | None = None):
        self.listed, self.reports = files, reports or {}

    def files(self, remote: str) -> list[str]:
        return [f for f in self.listed if f == remote or f.startswith(remote + "/")]

    def get(self, remote: str, local: Path) -> Path:
        target = local / remote
        if remote in self.reports:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(json.dumps(self.reports[remote]), encoding="utf-8")
        return target


def test_export_merges_the_supervised_adapter_and_then_the_reinforcement_learned_one(
    tmp_path: Path,
):
    work = tmp_path / "work"
    alone = stages.Run("r3", tmp_path, {}, Store(["runs/r1/sft/adapter/a"]), work)
    assert stages.adapters(alone, "r1") == {"sft": [work / "runs/r1/sft/adapter"]}
    store = Store(["runs/r2/rl/adapter/a"], {"runs/r2/rl/rl.json": {"sft": "r1"}})
    both = stages.adapters(stages.Run("r3", tmp_path, {}, store, work), "r2")
    assert both == {
        "sft": [work / "runs/r1/sft/adapter"],
        "rl": [work / "runs/r1/sft/adapter", work / "runs/r2/rl/adapter"],
    }


def test_export_follows_the_rejection_sampled_adapter_into_reinforcement_learning(tmp_path: Path):
    work = tmp_path / "work"
    reports = {
        "runs/r4/rft/rft.json": {"sft": "r2", "samples": "r4"},
        "runs/r4/rl/rl.json": {"start": {"run": "r4", "stage": "rft"}},
    }
    chained = Store(["runs/r4/rft/adapter/a", "runs/r4/rl/adapter/a"], reports)
    found = stages.adapters(stages.Run("r5", tmp_path, {}, chained, work), "r4")
    assert found == {
        "sft": [work / "runs/r2/sft/adapter"],
        "rft": [work / "runs/r4/rft/adapter"],
        "rl": [work / "runs/r4/rft/adapter", work / "runs/r4/rl/adapter"],
    }
    only = Store(["runs/r4/rft/adapter/a"], reports)
    assert stages.adapters(stages.Run("r5", tmp_path, {}, only, work), "r4") == {
        "sft": [work / "runs/r2/sft/adapter"],
        "rft": [work / "runs/r4/rft/adapter"],
    }


def test_export_also_gives_the_random_reward_twin_its_model(tmp_path: Path):
    work = tmp_path / "work"
    reports = {"runs/r4/rl/rl.json": {"start": {"run": "r4", "stage": "rft"}},
               "runs/r4/rft/rft.json": {"sft": "r2"}}  # fmt: skip
    files = ["runs/r4/rft/adapter/a", "runs/r4/rl/adapter/a", "runs/r4/rl-random/adapter/a"]
    found = stages.adapters(stages.Run("r5", tmp_path, {}, Store(files, reports), work), "r4")
    assert found["rl-random"] == [work / "runs/r4/rft/adapter", work / "runs/r4/rl-random/adapter"]


def test_a_later_stage_starts_from_the_rejection_sampled_adapter_when_there_is_one(tmp_path: Path):
    work = tmp_path / "work"
    with_rft = Store(["runs/r4/rft/adapter/a"])
    assert stages.starting(stages.Run("r5", tmp_path, {}, with_rft, work), "r4") == ("r4", "rft")
    sampled = Store(["runs/r4/sample/sample.json"], {"runs/r4/sample/sample.json": {"sft": "r2"}})
    assert stages.starting(stages.Run("r5", tmp_path, {}, sampled, work), "r4") == ("r2", "sft")
    assert stages.starting(stages.Run("r5", tmp_path, {}, Store([]), work), "r4") == ("r4", "sft")


def test_every_slot_gets_the_whole_context():
    found = validate.arguments(Path("llama-server"), Path("g.gguf"), 8091, 4)
    assert found[found.index("-c") + 1] == str(8192 * 4)
    assert found[found.index("-np") + 1] == "4"
