"""Asking the guide about a failure and reporting what it said (guide-evaluation R2.1-R2.7)."""

import http.server
import json
import threading
from pathlib import Path
from typing import ClassVar

import pytest

from lotml_harness.guide import evaluate
from lotml_harness.tasks import Case, Task
from lotml_harness.tasks.types import Prim

BROKEN = "fn add(a: int, b: int) -> int:\n    return a + bs\n"
WRONG = "fn add(a: int, b: int) -> int:\n    return a - b\n"
FIXED = "fn add(a: int, b: int) -> int:\n    return a + b\n"
ANSWER = {
    "locations": [{"path": "solution.lotml", "symbol": "add", "lines": [1, 2]}],
    "kind": "body",
    "edit": None,
}


class Guide(http.server.BaseHTTPRequestHandler):
    """A guide server answering every request with `answer`, one token, at probability 0.99."""

    answer: ClassVar[dict] = {}

    def do_POST(self):
        self.rfile.read(int(self.headers["Content-Length"]))
        content = json.dumps(self.answer)
        token = [{"token": content, "logprob": -0.01}]
        body = {"choices": [{"message": {"content": content}, "logprobs": {"content": token}}]}
        data = json.dumps(body).encode()
        self.send_response(200)
        self.send_header("Content-Length", str(len(data)))
        self.end_headers()
        self.wfile.write(data)

    def log_message(self, *_):
        pass


@pytest.fixture
def config(tmp_path: Path):
    Guide.answer = ANSWER
    server = http.server.ThreadingHTTPServer(("127.0.0.1", 0), Guide)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    (tmp_path / "records").mkdir()
    meta = {"problem": "humaneval/0", "split": "train"}
    (tmp_path / "records" / "train.jsonl").write_text(json.dumps({"meta": meta}) + "\n", "utf-8")
    path = tmp_path / "guide.toml"
    path.write_text(
        f'url = "http://127.0.0.1:{server.server_port}"\nmodel = "g"\nthreshold = 0.5\n'
        'answer = 256\nrun_candidates = false\nrenderer = 1\nrecords = "records"\n',
        encoding="utf-8",
    )
    yield path
    server.shutdown()


def test_a_failure_is_asked_through_the_guide_tool_and_scored_against_the_fix(config):
    failure = evaluate.Failure(
        "mbpp/2", "phase1", "m", "check", None, "solution.lotml", BROKEN, FIXED, ""
    )
    row = evaluate.judged(failure, config)
    assert row["truth"] == ["add"]
    assert row["guide"]["locations"] == ["add"]
    assert row["guide"]["silent"] is False
    assert row["guide"]["confidence"] > 0.98
    assert row["baseline"] == ["add"]
    assert (row["top1"], row["top3"], row["baseline_top1"]) == (True, True, True)
    assert row["seconds"] > 0
    assert set(row) == {
        "problem", "origin", "model", "kind", "truth", "guide", "baseline",
        "top1", "top3", "baseline_top1", "baseline_top3", "seconds",
    }  # fmt: skip
    assert "a + b" not in json.dumps(row), "a row holds no code"


def test_a_failing_hidden_test_is_asked_with_its_block_and_the_baseline_is_the_called_function(
    config,
):
    task = Task("mbpp/2", "mbpp", "add", [("a", Prim("int")), ("b", Prim("int"))], Prim("int"),
                "Add.", [Case([1, 2], 3)])  # fmt: skip
    failure = evaluate.Failure(
        "mbpp/2", "phase1", "m", "test", None, "solution.lotml", WRONG, FIXED,
        evaluate.hidden(task),
    )  # fmt: skip
    row = evaluate.judged(failure, config)
    assert row["baseline"] == ["add"]
    assert row["truth"] == ["add"]
    assert row["top1"]


def test_a_silent_guide_is_counted_and_the_report_gives_no_verdict_short_of_the_minimum():
    row = {
        "problem": "mbpp/2", "origin": "phase1", "model": "m", "kind": "check", "truth": ["add"],
        "guide": {"locations": [], "kind": None, "edit": False, "withheld": None, "silent": True,
                  "reason": "not-confident", "confidence": None},
        "baseline": ["add"], "top1": False, "top3": False, "baseline_top1": True,
        "baseline_top3": True, "seconds": 0.4,
    }  # fmt: skip
    setting = {"cpu": "x", "model_file": "g.gguf", "quantization": "Q4_K_M", "memory": 2**30}
    text = evaluate.markdown([row], "d" * 64, setting, "2026-10-06")
    assert "**No verdict:** 1 failures, 96 short" in text
    assert "Silent on 1 of 1 (not-confident 1)" in text
    assert "| compiler's pointer | 1/1" in text
    assert "peak memory 1024 MiB" in text
    assert "| check | 1 | 0% | 100% |" in text
