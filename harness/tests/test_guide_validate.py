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
    def __init__(self, files: list[str], rl_report: dict | None = None):
        self.listed, self.rl_report = files, rl_report

    def files(self, remote: str) -> list[str]:
        return [f for f in self.listed if f.startswith(remote + "/")]

    def get(self, remote: str, local: Path) -> Path:
        target = local / remote
        if remote.endswith("rl.json"):
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(json.dumps(self.rl_report), encoding="utf-8")
        return target


def test_export_merges_the_supervised_adapter_and_then_the_reinforcement_learned_one(
    tmp_path: Path,
):
    work = tmp_path / "work"
    alone = stages.Run("r3", tmp_path, {}, Store(["runs/r1/sft/adapter/a"]), work)
    assert stages.adapters(alone, "r1") == {"sft": [work / "runs/r1/sft/adapter"]}
    store = Store(["runs/r2/rl/adapter/a"], {"sft": "r1"})
    both = stages.adapters(stages.Run("r3", tmp_path, {}, store, work), "r2")
    assert both == {
        "sft": [work / "runs/r1/sft/adapter"],
        "rl": [work / "runs/r1/sft/adapter", work / "runs/r2/rl/adapter"],
    }
