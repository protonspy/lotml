"""HumanEval's and MBPP's original releases as agent tasks (specs/agent-humaneval/)."""

import gzip
import json
from hashlib import sha256

import pytest

from lotml_harness.agent import humaneval
from lotml_harness.tasks import sources

RECORDS = [
    {"task_id": "HumanEval/0", "entry_point": "f", "prompt": "", "canonical_solution": ""},
    {"task_id": "HumanEval/1", "entry_point": "g", "prompt": "", "canonical_solution": ""},
]


def published(monkeypatch, tmp_path, body: bytes) -> None:
    """Serve `body` as the pinned HumanEval file, with its digest pinned."""

    def download(url, target, limit=None, digest=None):
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_bytes(body)
        return target

    monkeypatch.setattr(sources, "CACHE", tmp_path)
    monkeypatch.setattr(sources, "download", download)
    monkeypatch.setattr(sources, "HUMAN_EVAL_SHA256", sha256(body).hexdigest())


def test_humaneval_reads_the_pinned_file_with_the_digest_of_the_bytes_it_parsed(
    tmp_path, monkeypatch
):
    body = gzip.compress("".join(json.dumps(r) + "\n" for r in RECORDS).encode())
    published(monkeypatch, tmp_path, body)
    records, digest = humaneval.read_humaneval()
    assert records == RECORDS
    assert digest == sha256(body).hexdigest()


def test_humaneval_refuses_a_file_that_decompresses_past_its_bound(tmp_path, monkeypatch):
    published(monkeypatch, tmp_path, gzip.compress(b" " * 1000))
    monkeypatch.setattr(humaneval, "DECOMPRESSED_LIMIT", 999)
    with pytest.raises(ValueError, match="999 bytes"):
        humaneval.read_humaneval()


def test_humaneval_pins_a_full_commit_and_a_sha_256():
    assert len(sources.HUMAN_EVAL) == 40
    assert len(sources.HUMAN_EVAL_SHA256) == 64
