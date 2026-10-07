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
    "locations": [{"path": "solution.lot", "symbol": "add", "lines": [1, 2]}],
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
        "mbpp/2", "phase1", "m", "check", None, "solution.lot", BROKEN, FIXED, ""
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
        "mbpp/2", "phase1", "m", "test", None, "solution.lot", WRONG, FIXED,
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
    assert "| check | 1 | 0/1 (0%, 95% CI 0% to 79%) |" in text
    assert "| 1/1 (100%, 95% CI 21% to 100%) | 1 | — |" in text


def test_calls_that_overrun_or_print_no_json_score_as_finding_nothing(
    monkeypatch: pytest.MonkeyPatch, tmp_path: Path
):
    failure = evaluate.Failure(
        "mbpp/2", "agent", "m", "check", None, "a.lotml", "fn f() -> int:\n", "", ""
    )
    for said, reason in (("deadline", "deadline"), (evaluate.UNREAD, "unread"), ([1], "unread")):
        monkeypatch.setattr(evaluate, "_call", lambda *_, said=said, **__: said)
        row = evaluate.judged(failure, tmp_path / "harness-guide.toml")
        assert (row["guide"]["silent"], row["guide"]["reason"]) == (True, reason)
        assert (row["truth"], row["baseline"], row["top1"]) == ([], [], False)


def test_every_breakdown_gives_top_3_silence_and_precision_with_intervals():
    row = {
        "problem": "mbpp/2", "origin": "agent", "model": "a|b", "kind": "test", "truth": ["add"],
        "guide": {"locations": ["add"], "kind": "body", "edit": False, "withheld": None,
                  "silent": False, "reason": None, "confidence": 0.9},
        "baseline": [], "top1": True, "top3": True, "baseline_top1": False,
        "baseline_top3": False, "seconds": 1.0,
    }  # fmt: skip
    setting = {"cpu": "x", "model_file": "g.gguf", "quantization": "Q4_K_M", "memory": None}
    text = evaluate.markdown([row], "d" * 64, setting, "2026-10-06")
    assert "| model that failed | failures | guide top-1 | guide top-3 |" in text
    assert "| a\\|b | 1 | 1/1 (100%, 95% CI 21% to 100%) | 1/1 (100%" in text
    assert "| 0/1 (0%, 95% CI 0% to 79%) | 0 | 1/1 (100%, 95% CI 21% to 100%) |" in text
