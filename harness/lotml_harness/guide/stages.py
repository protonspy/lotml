"""A training run inside its pod: the stages it names, in order, each reading its inputs from the
private repository and writing its outputs back, with the run's status after every step
(specs/training-pipeline/ R2.3, R2.4, R3.2-R3.5).

    python -m lotml_harness.guide.stages --llama-cpp <dir>

The run is described by the pod's environment (`guide/pod.py`). The local work directory mirrors
the repository — `runs/<run>/<stage>/…` here is `runs/<run>/<stage>/…` there — so a checkpoint
downloaded to resume lands where the trainer looks for it.
"""

import argparse
import datetime
import json
import os
import re
import tempfile
import traceback
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path

from lotml_harness.agent.secrets import anonymised, scrub
from lotml_harness.guide import hub as hubs
from lotml_harness.guide.hub import Hub

WORK = Path("/work")
CHECKPOINT = re.compile(r"checkpoint-(\d+)")


@dataclass
class Run:
    """What a stage needs: the run, where its records are, the run each stage reads from, the
    repository and the local mirror of it."""

    run: str
    records: Path
    inputs: dict[str, str]
    hub: Hub
    work: Path
    llama_cpp: Path | None = None
    done: list[str] = field(default_factory=list)

    def out(self, stage: str) -> Path:
        return self.work / "runs" / self.run / stage

    def source(self, stage: str) -> str:
        """The run whose outputs `stage` reads: the one named for it, else this one."""
        return self.inputs.get(stage, self.run)


def status(run: Run, stage: str, state: str, error: str | None = None) -> None:
    """Write `runs/<run>/status.json`, which the machine that started the pod watches."""
    found = {
        "run": run.run, "stage": stage, "state": state, "done": run.done,
        "at": datetime.datetime.now(datetime.UTC).isoformat(),
        "error": None if error is None else anonymised(scrub(error))[-2000:],
    }  # fmt: skip
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "status.json"
        path.write_text(json.dumps(found, indent=2), encoding="utf-8")
        run.hub.put(path, f"runs/{run.run}/status.json", f"run {run.run}: {stage} {state}")


def latest(names: list[str], under: str) -> str | None:
    """The newest checkpoint directory among repository files under `under`."""
    steps = {
        int(match.group(1)): f"{under}/{match.group(0)}"
        for name in names
        if (match := CHECKPOINT.match(name.removeprefix(under + "/").split("/")[0]))
    }
    return steps[max(steps)] if steps else None


def resume(run: Run, stage: str) -> Path | None:
    """Download the stage's latest checkpoint from the repository into its place in the mirror,
    for the trainer to resume from (R2.3); None when the run has none."""
    under = f"runs/{run.run}/{stage}/checkpoints"
    found = latest(run.hub.files(under), under)
    return None if found is None else run.hub.get(found, run.work)


def upload_latest(run: Run, stage: str) -> str | None:
    """Upload the stage's newest local checkpoint; its remote path, or None when there is none."""
    local = run.out(stage) / "checkpoints"
    names = [p.name for p in local.glob("checkpoint-*") if p.is_dir()]
    under = f"runs/{run.run}/{stage}/checkpoints"
    found = latest([f"{under}/{n}" for n in names], under)
    if found is None:
        return None
    run.hub.put(run.work / found, found, f"run {run.run}: {stage} {found.rsplit('/', 1)[-1]}")
    return found


def uploader(run: Run, stage: str) -> object:
    """A trainer callback that uploads each checkpoint as the trainer saves it."""
    from transformers import TrainerCallback

    class Upload(TrainerCallback):
        def on_save(self, args, state, control, **kwargs):
            upload_latest(run, stage)

    return Upload()


def sft(run: Run) -> None:
    """Fine-tune the base model on the train split, the loss on the answer alone (R3.2)."""
    from lotml_harness.guide import train

    out = run.out("sft")
    resume(run, "sft")
    report = train.train(train.Settings(), run.records, out, callbacks=[uploader(run, "sft")])
    (out / "train.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    run.hub.put(out / "adapter", f"runs/{run.run}/sft/adapter", f"run {run.run}: sft adapter")
    run.hub.put(out / "train.json", f"runs/{run.run}/sft/train.json", f"run {run.run}: sft report")


STAGES: dict[str, Callable[[Run], None]] = {"sft": sft}


def execute(run: Run, stages: list[str], known: dict[str, Callable[[Run], None]]) -> None:
    """Run `stages` in order, the run's status written before and after each; on a failure the
    status says which stage and why, and the failure is raised again for the pod to end."""
    unknown = [s for s in stages if s not in known]
    if unknown:
        raise ValueError(f"unknown stages: {', '.join(unknown)}")
    for stage in stages:
        status(run, stage, "running")
        try:
            known[stage](run)
        except Exception:
            status(run, stage, "failed", traceback.format_exc())
            raise
        run.done.append(stage)
    status(run, stages[-1] if stages else "", "done")


def records(store: Hub, digest: str, work: Path) -> Path:
    """The run's records from the repository, checked against the digest they are named by."""
    local = store.get(f"records/{digest}", work)
    found = hubs.digest(local)
    if found != digest:
        raise ValueError(f"the records downloaded hash to {found}, not {digest}")
    return local


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--llama-cpp", type=Path)
    args = parser.parse_args(argv)
    env = os.environ
    store = Hub(env["LOTML_HUB"])
    inputs = dict(p.split("=", 1) for p in env.get("LOTML_INPUTS", "").split(",") if p)
    run = Run(
        run=env["LOTML_RUN"], records=records(store, env["LOTML_RECORDS"], WORK), inputs=inputs,
        hub=store, work=WORK, llama_cpp=args.llama_cpp,
    )  # fmt: skip
    execute(run, [s for s in env["LOTML_STAGES"].split(",") if s], STAGES)


if __name__ == "__main__":
    main()
