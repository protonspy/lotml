"""The training pipeline from this machine, against a fake RunPod and a fake repository
(specs/training-pipeline/ R1.2-R1.4, R2.5, R3.1, R3.5, R5.1, R5.2)."""

import datetime
import json
from decimal import Decimal
from pathlib import Path

import pytest

from lotml_harness import split
from lotml_harness.guide import ledger as ledgers
from lotml_harness.guide import pipeline
from lotml_harness.guide.ledger import CapExceeded, Ledger
from lotml_harness.guide.pipeline import Plan, Refused

COMMIT = "0123456789abcdef0123456789abcdef01234567"
STATE = {"task": None, "path": "solution.lotml", "text": "x", "diagnostics": [], "failing": None}


def problem(bucket: str) -> str:
    return next(f"humaneval/{n}" for n in range(164) if split.split(f"humaneval/{n}") == bucket)


def records(directory: Path, state: bool = True) -> Path:
    directory.mkdir(parents=True)
    for bucket in ("train", "validation"):
        record = {"messages": [], "meta": {"problem": problem(bucket), "split": bucket}}
        if state:
            record["state"] = STATE
        (directory / f"{bucket}.jsonl").write_text(json.dumps(record) + "\n", encoding="utf-8")
    return directory


class Store:
    """A repository holding the run's status and reports in memory."""

    def __init__(self, statuses: list[dict | None] | None = None, reports: dict | None = None):
        self.statuses, self.reports, self.puts = list(statuses or []), reports or {}, []

    def put_records(self, path: Path) -> str:
        self.puts.append("records")
        return "d1"

    def put_lineage(self, run, records, inputs, commit) -> None:
        self.puts.append("lineage")
        self.reports["run.json"] = {
            "run": run,
            "records": records,
            "inputs": inputs,
            "commit": commit,
        }

    def files(self, remote: str) -> list[str]:
        name = remote.split("/", 2)[-1]
        if name == "status.json":
            if self.statuses and self.statuses[0] is None:
                self.statuses.pop(0)
                return []
            return [remote] if self.statuses else []
        return [remote] if name in self.reports else []

    def get(self, remote: str, local: Path) -> Path:
        name = remote.split("/", 2)[-1]
        body = self.statuses.pop(0) if name == "status.json" else self.reports[name]
        path = local / remote
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(body), encoding="utf-8")
        return path


@pytest.fixture
def fake(monkeypatch: pytest.MonkeyPatch, tmp_path: Path):
    calls: dict[str, list] = {"create": [], "terminate": []}
    monkeypatch.setenv("HF_TOKEN", "hf_secret_token_0123456789")
    monkeypatch.setattr(pipeline.runpod, "price", lambda gpu, cloud: 0.34)
    monkeypatch.setattr(
        pipeline.runpod, "create", lambda r: calls["create"].append(r) or {"id": "pod_1"}
    )
    monkeypatch.setattr(pipeline.runpod, "status", lambda pod: {"status": "RUNNING"})
    monkeypatch.setattr(pipeline.runpod, "terminate", lambda pod: calls["terminate"].append(pod))
    monkeypatch.setattr(pipeline, "REPORTS", tmp_path / "training")
    monkeypatch.setattr(pipeline, "RUNS", tmp_path / "runs")
    monkeypatch.setattr(ledgers, "REPORT", tmp_path / "runpod.md")
    return calls


def plan(tmp_path: Path, stages: list[str] | None = None, **kwargs) -> Plan:
    found = kwargs.pop("records", None) or records(tmp_path / "records")
    return Plan(
        stages or ["sft", "rl"], "me/guide", "NVIDIA GeForce RTX 4090", 2, records=found, **kwargs
    )


def test_a_run_is_refused_before_any_pod_exists(fake, tmp_path: Path):
    ledger = Ledger(tmp_path / "runpod.jsonl")
    yes = lambda _: True  # noqa: E731
    with pytest.raises(Refused, match="unknown stages: ppo"):
        pipeline.start(plan(tmp_path, ["ppo"]), Store(), ledger, COMMIT, on_github=yes)
    old = records(tmp_path / "old", state=False)
    with pytest.raises(Refused, match="build them again"):
        pipeline.start(plan(tmp_path, records=old), Store(), ledger, COMMIT, on_github=yes)
    with pytest.raises(Refused, match="on no remote branch"):
        pipeline.start(plan(tmp_path / "a"), Store(), ledger, COMMIT, on_github=lambda _: False)
    with pytest.raises(CapExceeded):
        pipeline.start(
            plan(tmp_path / "b", cap=Decimal("0.50")), Store(), ledger, COMMIT, on_github=yes
        )
    assert fake["create"] == []


def test_a_dry_run_prints_the_request_with_the_token_hidden(fake, tmp_path: Path, capsys):
    store = Store()
    found = pipeline.start(
        plan(tmp_path),
        store,
        Ledger(tmp_path / "l.jsonl"),
        COMMIT,
        dry=True,
        on_github=lambda _: True,
    )
    assert found is None and fake["create"] == [] and store.puts == []
    printed = json.loads(capsys.readouterr().out)
    assert printed["estimate_usd"] == "0.6800"
    assert printed["request"]["env"]["HF_TOKEN"] == "<redacted>"  # noqa: S105 - the placeholder


def test_a_run_is_watched_terminated_recorded_and_reported(fake, tmp_path: Path, monkeypatch):
    monkeypatch.setattr(pipeline, "head", lambda: COMMIT)
    monkeypatch.setattr(pipeline, "pushed", lambda _: True)
    monkeypatch.setattr(pipeline.time, "sleep", lambda _: None)
    metrics = {
        "records": 2,
        "within_schema": 1.0,
        "top1": 1.0,
        "top3": 1.0,
        "edits_pass": 0.5,
        "silent": {},
    }
    export = {"models": {"sft": metrics, "rl": metrics | {"edits_pass": 1.0}}, "final": "rl",
              "threshold": 0.8, "precision": 1.0, "shown": 1.0, "target": 0.9}  # fmt: skip
    store = Store(
        [None, {"stage": "sft", "state": "running"}, {"stage": "export", "state": "done"}],
        {
            "report.json": export,
            "rl/rl.json": {"reward_first": 0.2, "reward_last": 0.6, "unjudged": 1},
        },
    )
    ledger = Ledger(tmp_path / "runpod.jsonl")
    path = pipeline.run(plan(tmp_path, ["sft", "rl", "export"]), store, ledger)
    assert fake["terminate"] == ["pod_1"]
    assert store.puts == ["records", "lineage"]
    [row] = ledger.rows()
    assert row["ended"] is not None and row["pod"] == "pod_1"
    assert fake["create"][0]["env"]["LOTML_RECORDS"] == "d1"
    text = path.read_text(encoding="utf-8")
    assert "| rl | 2 | 100.0% | 100.0% | 100.0% | 100.0% | none |" in text
    assert "Ended: done in export." in text
    assert "Reward: 0.2 at the start, 0.6 at the end; 1 answers" in text
    assert "hf_secret_token_0123456789" not in text
    assert (tmp_path / "runpod.md").exists()


def test_a_failure_while_watching_still_terminates_and_records_the_pod(
    fake, tmp_path: Path, monkeypatch
):
    monkeypatch.setattr(pipeline, "head", lambda: COMMIT)
    monkeypatch.setattr(pipeline, "pushed", lambda _: True)

    def broken(store, run):
        raise ConnectionError("the network went away")

    monkeypatch.setattr(pipeline, "status", broken)
    ledger = Ledger(tmp_path / "runpod.jsonl")
    with pytest.raises(ConnectionError):
        pipeline.run(plan(tmp_path), Store(), ledger)
    assert fake["terminate"] == ["pod_1"]
    assert ledger.rows()[0]["ended"] is not None


def test_watching_stops_when_the_pod_is_gone(fake, tmp_path: Path, monkeypatch):
    monkeypatch.setattr(pipeline.runpod, "status", lambda pod: None)
    deadline = pipeline.now() + datetime.timedelta(hours=1)
    said = []
    assert (
        pipeline.watch(Store(), "r1", "pod_1", deadline, sleep=lambda _: None, say=said.append)
        is None
    )
    assert said == ["r1: pod pod_1 is gone"]


def test_reconcile_closes_open_rows(fake, tmp_path: Path, monkeypatch):
    ledger = Ledger(tmp_path / "runpod.jsonl")
    started = pipeline.now() - datetime.timedelta(hours=5)
    ledger.open("r1", "gone", ["sft"], "g", "COMMUNITY", Decimal("1.00"), 2, started)
    ledger.open("r2", "here", ["sft"], "g", "COMMUNITY", Decimal("1.00"), 2, started)
    monkeypatch.setattr(
        pipeline.runpod, "status", lambda pod: None if pod == "gone" else {"status": "RUNNING"}
    )
    pipeline.reconcile(ledger)
    gone, here = ledger.rows()
    assert gone["minutes"] == 120, "a pod gone is closed at its deadline"
    assert fake["terminate"] == ["here"] and here["ended"] is not None
