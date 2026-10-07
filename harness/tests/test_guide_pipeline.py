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
    monkeypatch.setattr(pipeline.runpod, "pods", list)
    monkeypatch.setattr(pipeline, "REPORTS", tmp_path / "training")
    monkeypatch.setattr(pipeline, "RUNS", tmp_path / "runs")
    monkeypatch.setattr(ledgers, "REPORT", tmp_path / "runpod.md")
    return calls


def plan(tmp_path: Path, stages: list[str] | None = None, **kwargs) -> Plan:
    found = kwargs.pop("records", None) or records(tmp_path / "records")
    hours = kwargs.pop("hours", 2)
    gpu = "NVIDIA GeForce RTX 4090"
    return Plan(stages or ["sft", "rl"], "me/guide", gpu, hours, records=found, **kwargs)


def test_a_run_is_refused_before_any_pod_exists(fake, tmp_path: Path):
    ledger = Ledger(tmp_path / "runpod.jsonl", tmp_path / "committed.jsonl")
    yes = lambda _: True  # noqa: E731
    with pytest.raises(Refused, match="unknown stages: ppo"):
        pipeline.prepare(plan(tmp_path, ["ppo"]), Store(), ledger, COMMIT, on_github=yes)
    old = records(tmp_path / "old", state=False)
    with pytest.raises(Refused, match="build them again"):
        pipeline.prepare(plan(tmp_path, records=old), Store(), ledger, COMMIT, on_github=yes)
    with pytest.raises(Refused, match="on no remote branch"):
        pipeline.prepare(plan(tmp_path / "a"), Store(), ledger, COMMIT, on_github=lambda _: False)
    with pytest.raises(CapExceeded):
        pipeline.prepare(
            plan(tmp_path / "b", cap=Decimal("0.50")), Store(), ledger, COMMIT, on_github=yes
        )
    assert fake["create"] == []


def test_a_dry_run_prints_the_request_with_the_token_hidden(fake, tmp_path: Path, capsys):
    store = Store()
    found = pipeline.prepare(
        plan(tmp_path),
        store,
        Ledger(tmp_path / "l.jsonl", None),
        COMMIT,
        dry=True,
        on_github=lambda _: True,
    )
    assert found is None and fake["create"] == [] and store.puts == []
    printed = json.loads(capsys.readouterr().out)
    assert printed["estimate_usd"] == "0.7650", "the deadline, its grace and the termination"
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
    ledger = Ledger(tmp_path / "runpod.jsonl", tmp_path / "committed.jsonl")
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


def test_a_look_that_fails_is_looked_again_at_and_the_pod_still_ends_recorded(
    fake, tmp_path: Path, monkeypatch
):
    monkeypatch.setattr(pipeline, "head", lambda: COMMIT)
    monkeypatch.setattr(pipeline, "pushed", lambda _: True)
    monkeypatch.setattr(pipeline.time, "sleep", lambda _: None)
    looks = []

    def flaky(store, run):
        looks.append(run)
        if len(looks) == 1:
            raise ConnectionError("the network went away")
        return None

    monkeypatch.setattr(pipeline, "status", flaky)
    monkeypatch.setattr(pipeline.runpod, "status", lambda pod: None if len(looks) > 2 else {})
    ledger = Ledger(tmp_path / "runpod.jsonl", tmp_path / "committed.jsonl")
    pipeline.run(plan(tmp_path), Store(), ledger)
    assert fake["terminate"] == ["pod_1"]
    assert ledger.rows()[0]["ended"] is not None


def test_a_pod_whose_creation_answer_was_lost_is_found_by_name_and_ended(
    fake, tmp_path: Path, monkeypatch
):
    monkeypatch.setattr(pipeline, "head", lambda: COMMIT)
    monkeypatch.setattr(pipeline, "pushed", lambda _: True)

    def lost(request):
        raise pipeline.runpod.Transient("RunPod could not be reached: TimeoutError")

    monkeypatch.setattr(pipeline.runpod, "create", lost)
    monkeypatch.setattr(
        pipeline.runpod, "named", lambda name: "pod_7" if name.startswith("lotml-guide-") else None
    )
    monkeypatch.setattr(pipeline, "watch", lambda *_, **__: None)
    ledger = Ledger(tmp_path / "runpod.jsonl", tmp_path / "committed.jsonl")
    pipeline.run(plan(tmp_path), Store(), ledger)
    assert fake["terminate"] == ["pod_7"]
    assert ledger.rows()[0]["pod"] == "pod_7" and ledger.rows()[0]["ended"] is not None


def test_a_ledger_that_fails_right_after_creation_still_gets_the_pod_ended_and_recorded(
    fake, tmp_path: Path, monkeypatch
):
    monkeypatch.setattr(pipeline, "head", lambda: COMMIT)
    monkeypatch.setattr(pipeline, "pushed", lambda _: True)
    ledger = Ledger(tmp_path / "runpod.jsonl", tmp_path / "committed.jsonl")
    real, calls = ledger.open, []

    def once_broken(*args):
        calls.append(args)
        if len(calls) == 1:
            raise OSError("disk full")
        return real(*args)

    monkeypatch.setattr(ledger, "open", once_broken)
    with pytest.raises(OSError, match="disk full"):
        pipeline.run(plan(tmp_path), Store(), ledger)
    assert fake["terminate"] == ["pod_1"]
    [row] = ledger.rows()
    assert row["ended"] is not None


def test_a_pod_that_will_not_go_keeps_its_row_open_and_says_so(fake, tmp_path: Path, monkeypatch):
    monkeypatch.setattr(pipeline, "head", lambda: COMMIT)
    monkeypatch.setattr(pipeline, "pushed", lambda _: True)
    monkeypatch.setattr(pipeline, "watch", lambda *_, **__: None)

    def stuck(pod):
        raise pipeline.runpod.RunPodError("pod pod_1 still exists 300 s after it was terminated")

    monkeypatch.setattr(pipeline.runpod, "terminate", stuck)
    ledger = Ledger(tmp_path / "runpod.jsonl", tmp_path / "committed.jsonl")
    with pytest.raises(pipeline.runpod.RunPodError, match="still exists"):
        pipeline.run(plan(tmp_path), Store(), ledger)
    assert ledger.rows()[0]["ended"] is None, "counted at its full deadline until reconciled"
    assert "open, counted at its" in (tmp_path / "runpod.md").read_text(encoding="utf-8")


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
    ledger = Ledger(tmp_path / "runpod.jsonl", tmp_path / "committed.jsonl")
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


def test_a_deadline_must_be_positive_and_bounded(fake, tmp_path: Path):
    ledger = Ledger(tmp_path / "runpod.jsonl", None)
    for hours in (0, -1, 13):
        with pytest.raises(Refused, match="deadline"):
            pipeline.prepare(
                plan(tmp_path / f"h{hours}", hours=hours),
                Store(),
                ledger,
                COMMIT,
                on_github=lambda _: True,
            )


def test_reconcile_ends_and_records_a_pod_it_named_that_the_ledger_never_saw(
    fake, tmp_path: Path, monkeypatch
):
    ledger = Ledger(tmp_path / "runpod.jsonl", tmp_path / "committed.jsonl")
    orphan = {
        "id": "pod_8",
        "name": "lotml-guide-20261007-1200",
        "cost": 0.34,
        "createdAt": "2026-10-07T12:00:00Z",
        "gpu": {"id": "NVIDIA GeForce RTX 4090"},
    }
    other = {"id": "pod_9", "name": "someone-else"}
    monkeypatch.setattr(pipeline.runpod, "pods", lambda: [orphan, other])
    monkeypatch.setattr(pipeline.runpod, "status", lambda pod: None)
    pipeline.reconcile(ledger)
    assert fake["terminate"] == ["pod_8"]
    [row] = ledger.rows()
    assert (row["pod"], row["run"], row["gpu"]) == (
        "pod_8",
        "20261007-1200",
        "NVIDIA GeForce RTX 4090",
    )
    assert row["ended"] is not None
    assert {r["pod"] for r in Ledger(tmp_path / "committed.jsonl", None).rows()} == {"pod_8"}
