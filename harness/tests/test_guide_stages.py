"""A training run inside its pod: stages in order, status after each, checkpoints uploaded and
resumed (specs/training-pipeline/ R2.3, R2.4, R3.5)."""

import json
from pathlib import Path

import pytest

from lotml_harness.guide import stages
from lotml_harness.guide.stages import Run, execute, latest, resume, upload_latest


class Store:
    """A Hub that keeps puts and serves `files` from memory, and `get` from a local mirror."""

    def __init__(self, files: tuple[str, ...] = (), mirror: Path | None = None):
        self.listed, self.mirror, self.puts, self.statuses = list(files), mirror, [], []

    def put(self, local: Path, remote: str, message: str) -> None:
        self.puts.append(remote)
        if remote.endswith("status.json"):
            self.statuses.append(json.loads(local.read_text(encoding="utf-8")))

    def files(self, remote: str) -> list[str]:
        return [f for f in self.listed if f.startswith(remote + "/")]

    def get(self, remote: str, local: Path) -> Path:
        target = local / remote
        target.mkdir(parents=True, exist_ok=True)
        (target / "fetched").write_text("x", encoding="utf-8")
        return target


def run(tmp_path: Path, store: Store, inputs: dict[str, str] | None = None) -> Run:
    return Run("r2", tmp_path / "records", inputs or {}, store, tmp_path / "work")


def test_the_latest_checkpoint_is_the_highest_step():
    under = "runs/r2/sft/checkpoints"
    names = [f"{under}/checkpoint-50/a", f"{under}/checkpoint-150/a", f"{under}/checkpoint-100/b"]
    assert latest(names, under) == f"{under}/checkpoint-150"
    assert latest([], under) is None


def test_a_stage_resumes_from_its_run_s_latest_checkpoint_in_the_mirror(tmp_path: Path):
    under = "runs/r2/sft/checkpoints"
    store = Store((f"{under}/checkpoint-50/x", f"{under}/checkpoint-100/x"))
    found = resume(run(tmp_path, store), "sft")
    assert found == tmp_path / "work" / under / "checkpoint-100"
    assert (run(tmp_path, store).out("sft") / "checkpoints" / "checkpoint-100" / "fetched").exists()
    assert resume(run(tmp_path, Store()), "sft") is None


def test_the_newest_local_checkpoint_is_uploaded(tmp_path: Path):
    store = Store()
    current = run(tmp_path, store)
    for step in (50, 100):
        (current.out("sft") / "checkpoints" / f"checkpoint-{step}").mkdir(parents=True)
    assert upload_latest(current, "sft") == "runs/r2/sft/checkpoints/checkpoint-100"
    assert store.puts == ["runs/r2/sft/checkpoints/checkpoint-100"]


def test_stages_run_in_order_with_their_status(tmp_path: Path):
    store, seen = Store(), []
    known = {
        "sft": lambda r: seen.append(("sft", r.source("sft"))),
        "rl": lambda r: seen.append(("rl", r.source("rl"))),
    }
    execute(run(tmp_path, store, {"rl": "r1"}), ["sft", "rl"], known)
    assert seen == [("sft", "r2"), ("rl", "r1")]
    assert [(s["stage"], s["state"]) for s in store.statuses] == [
        ("sft", "running"), ("rl", "running"), ("rl", "done"),
    ]  # fmt: skip
    assert store.statuses[-1]["done"] == ["sft", "rl"]


def test_a_failing_stage_says_so_and_stops_the_run(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setenv("HF_TOKEN", "hf_secret_value_123456")
    store = Store()

    def broken(_: Run) -> None:
        raise RuntimeError("out of memory with hf_secret_value_123456")

    with pytest.raises(RuntimeError):
        execute(run(tmp_path, store), ["sft", "rl"], {"sft": broken, "rl": lambda r: None})
    last = store.statuses[-1]
    assert (last["stage"], last["state"]) == ("sft", "failed")
    assert "out of memory" in last["error"] and "hf_secret_value_123456" not in last["error"]
    with pytest.raises(ValueError, match="unknown stages: ppo"):
        execute(run(tmp_path, store), ["ppo"], stages.STAGES)


def test_records_are_checked_against_their_digest(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setattr(stages.hubs, "digest", lambda _: "other")
    with pytest.raises(ValueError, match="hash to other, not abc"):
        stages.records(Store(), "abc", tmp_path)


def test_a_status_that_cannot_be_written_does_not_hide_the_stage_s_failure(tmp_path: Path):
    class Broken(Store):
        def put(self, local, remote, message):
            if "failed" in message:
                raise ConnectionError("the hub went away")

    def crashed(_: Run) -> None:
        raise RuntimeError("the real failure")

    with pytest.raises(RuntimeError, match="the real failure"):
        execute(run(tmp_path, Broken()), ["sft"], {"sft": crashed})


def test_diagnostics_hold_the_gpu_setup_and_no_secret(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setenv("HF_TOKEN", "hf_secret_value_123456")
    monkeypatch.setenv("CUDA_VISIBLE_DEVICES", "0")
    monkeypatch.setattr(stages, "_output", lambda command, env=None: " ".join(command)[:20])
    found = stages.diagnostics()
    assert set(found) == {
        "nvidia-smi",
        "environment",
        "libcuda",
        "torch",
        "torch_without_ld_library_path",
    }
    assert found["environment"]["CUDA_VISIBLE_DEVICES"] == "0"
    assert "HF_TOKEN" not in found["environment"]


def test_a_failed_stage_leaves_its_diagnostics(tmp_path: Path, monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setattr(stages, "diagnostics", lambda: {"nvidia-smi": "no devices"})
    store = Store()

    def crashed(_: Run) -> None:
        raise RuntimeError("CUDA unknown error")

    with pytest.raises(RuntimeError):
        execute(run(tmp_path, store), ["sft"], {"sft": crashed})
    assert "runs/r2/diagnostics.json" in store.puts
    assert store.statuses[-1]["state"] == "failed"
