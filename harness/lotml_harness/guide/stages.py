"""A training run inside its pod: the stages it names, in order, each reading its inputs from the
private repository and writing its outputs back, with the run's status after every step
(specs/training-pipeline/ R2.3, R2.4, R3.2-R3.5).

    python -m lotml_harness.guide.stages --llama-cpp <dir>

The run is described by the pod's environment (`guide/pod.py`). The local work directory mirrors
the repository — `runs/<run>/<stage>/…` here is `runs/<run>/<stage>/…` there — so a checkpoint
downloaded to resume lands where the trainer looks for it.
"""

import argparse
import contextlib
import dataclasses
import datetime
import json
import os
import re
import subprocess
import sys
import tempfile
import traceback
from collections.abc import Callable
from dataclasses import dataclass, field
from pathlib import Path

from lotml_harness.agent.secrets import anonymised, scrub, scrub_value
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


def report(run: Run, source: str, stage: str) -> dict:
    """The JSON report `source`'s `stage` left, as `rl/rl.json` or `sample/sample.json`."""
    path = run.hub.get(f"runs/{source}/{stage}/{stage}.json", run.work)
    return json.loads(path.read_text(encoding="utf-8"))


def starting(run: Run, source: str) -> tuple[str, str]:
    """The run and stage whose adapter `source`'s later stages start from: its rejection-sampled
    one when it has one, else the supervised one of the run it sampled from, else its own."""
    if run.hub.files(f"runs/{source}/rft/adapter"):
        return source, "rft"
    if run.hub.files(f"runs/{source}/sample/sample.json"):
        return report(run, source, "sample")["sft"], "sft"
    return source, "sft"


def rft(run: Run) -> None:
    """Train the supervised guide further on its own sampled answers that passed, the record's
    target where none did (R3.7)."""
    from lotml_harness.guide import rft as rejection

    source = run.source("rft")
    supervised = report(run, source, "sample")["sft"]
    samples = run.hub.get(f"runs/{source}/sample/samples.jsonl", run.work)
    rows = [json.loads(line) for line in samples.read_text(encoding="utf-8").splitlines() if line]
    adapter = run.hub.get(f"runs/{supervised}/sft/adapter", run.work)
    out = run.out("rft")
    resume(run, "rft")
    found = rejection.fit(
        rejection.Settings(), rows, run.records, adapter, out, callbacks=[uploader(run, "rft")]
    )
    found |= {"samples": source, "sft": supervised}
    (out / "rft.json").write_text(json.dumps(found, indent=2), encoding="utf-8")
    run.hub.put(out / "adapter", f"runs/{run.run}/rft/adapter", f"run {run.run}: rft adapter")
    run.hub.put(out / "rft.json", f"runs/{run.run}/rft/rft.json", f"run {run.run}: rft report")


def rl(run: Run) -> None:
    """Train the guide further by group-relative policy optimization, the compiler's judgment its
    reward, from the adapter its source's earlier stages left (R3.3)."""
    from lotml_harness.guide import grpo

    begun, stage = starting(run, run.source("rl"))
    adapter = run.hub.get(f"runs/{begun}/{stage}/adapter", run.work)
    out = run.out("rl")
    resume(run, "rl")
    found = grpo.fit(grpo.Settings(), run.records, adapter, out, callbacks=[uploader(run, "rl")])
    found["start"] = {"run": begun, "stage": stage}
    (out / "rl.json").write_text(json.dumps(found, indent=2), encoding="utf-8")
    run.hub.put(out / "adapter", f"runs/{run.run}/rl/adapter", f"run {run.run}: rl adapter")
    run.hub.put(out / "rl.json", f"runs/{run.run}/rl/rl.json", f"run {run.run}: rl report")


def sampling(run: Run) -> None:
    """Sample several answers to every train record from the fine-tuned guide and judge each
    (R3.6): `runs/<run>/sample/samples.jsonl`, which rejection-sampled fine-tuning and the
    reinforcement-learning pool read."""
    from lotml_harness.guide import sample, train

    base = train.Settings()
    adapter = run.hub.get(f"runs/{run.source('sample')}/sft/adapter", run.work)
    model, tokenizer = sample.load(base.model, base.revision, [adapter])
    settings = sample.Settings(workers=max(2, (os.cpu_count() or 4) - 2))
    out = run.out("sample") / "samples.jsonl"
    rows = sample.sample(model, tokenizer, train.load(run.records, "train"), settings, out)
    summary = {
        "settings": dataclasses.asdict(settings),
        "records": len(rows),
        "sft": run.source("sample"),
        "mean": sum(r["mean"] for r in rows) / len(rows) if rows else 0.0,
        "always": sum(1 for r in rows if r["mean"] == 1.0),
        "never": sum(1 for r in rows if r["mean"] == 0.0),
        "unjudged": sum(1 for r in rows for a in r["answers"] if not a["judged"]),
    } | sample.pass_rates(rows, ks=(1, settings.answers))
    (out.parent / "sample.json").write_text(json.dumps(summary, indent=2), encoding="utf-8")
    run.hub.put(out.parent, f"runs/{run.run}/sample", f"run {run.run}: sampled answers")


def adapters(run: Run, source: str) -> dict[str, list[Path]]:
    """The models `source`'s adapters make, each as the adapters merged in order: the supervised
    one; the rejection-sampled one, which continues it; and the reinforcement-learned one on top of
    whichever it started from."""

    def adapter(at: str, stage: str) -> Path:
        return run.hub.get(f"runs/{at}/{stage}/adapter", run.work)

    found: dict[str, list[Path]] = {}
    if run.hub.files(f"runs/{source}/rl/adapter"):
        rl = report(run, source, "rl")
        begun = rl.get("start") or {"run": rl["sft"], "stage": "sft"}
        if begun["stage"] == "rft":
            found["sft"] = [adapter(report(run, begun["run"], "rft")["sft"], "sft")]
        found[begun["stage"]] = [adapter(begun["run"], begun["stage"])]
        found["rl"] = [*found[begun["stage"]], adapter(source, "rl")]
    elif run.hub.files(f"runs/{source}/rft/adapter"):
        found["sft"] = [adapter(report(run, source, "rft")["sft"], "sft")]
        found["rft"] = [adapter(source, "rft")]
    else:
        found["sft"] = [adapter(source, "sft")]
    return found


def export(run: Run) -> None:
    """Each model as a Q4_K_M GGUF file, asked about the validation split through the guide tool,
    and the threshold calibrated on the last one — the reinforcement-learned guide when there is
    one, so the report has the split before and after it (R3.4, R5.1)."""
    from lotml_harness.guide import train, validate

    if run.llama_cpp is None:
        raise ValueError("the export stage needs --llama-cpp")
    source = run.source("export")
    out = run.out("export")
    report: dict = {"run": run.run, "source": source, "models": {}}
    rows: list[dict] = []
    for name, merged_from in adapters(run, source).items():
        base = train.Settings()
        merged = train.merge(base.model, base.revision, merged_from, out / name / "merged")
        gguf = train.export(merged, run.llama_cpp, out / name)
        rows, found = validate.validate(gguf, run.llama_cpp, run.records)
        report["models"][name] = found
        run.hub.put(
            gguf, f"runs/{run.run}/export/{name}/{gguf.name}", f"run {run.run}: {name} gguf"
        )
        report["final"] = name
    threshold, precision, shown = validate.calibrated(rows)
    report |= {
        "threshold": threshold,
        "precision": precision,
        "shown": shown,
        "target": validate.PRECISION,
    }
    (out / "report.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    run.hub.put(out / "report.json", f"runs/{run.run}/report.json", f"run {run.run}: report")


GPU_ENVIRONMENT = re.compile(r"^(CUDA|NVIDIA|LD_LIBRARY_PATH$|PATH$)")
"""The variables a GPU's setup reads: none of them holds a secret."""
PROBE = (
    "import torch; print(torch.__version__, torch.version.cuda); "
    "print('available', torch.cuda.is_available()); print(torch.cuda.get_device_name(0))"
)


def _output(command: list[str], env: dict[str, str] | None = None) -> str:
    try:
        done = subprocess.run(  # noqa: S603
            command, capture_output=True, text=True, timeout=120, env=env, check=False
        )
    except (OSError, subprocess.SubprocessError) as error:
        return f"{type(error).__name__}: {error}"
    return (done.stdout + done.stderr)[-4000:]


def diagnostics() -> dict:
    """What the pod's GPU setup looks like, for a failure that names no cause: the driver and the
    card, the variables a GPU's setup reads, where libcuda comes from, and whether torch reaches
    the GPU with LD_LIBRARY_PATH as the image sets it and without it."""
    without = {k: v for k, v in os.environ.items() if k != "LD_LIBRARY_PATH"}
    return {
        "nvidia-smi": _output(["nvidia-smi"]),
        "environment": {k: v for k, v in os.environ.items() if GPU_ENVIRONMENT.match(k)},
        "libcuda": _output(
            ["bash", "-c", "ldconfig -p | grep -i libcuda; ls -la /usr/local/cuda/compat 2>&1"]
        ),
        "torch": _output([sys.executable, "-c", PROBE]),
        "torch_without_ld_library_path": _output([sys.executable, "-c", PROBE], without),
    }


def diagnose(run: Run) -> None:
    """Upload `runs/<run>/diagnostics.json`, scrubbed: a stage of its own, and what a failed stage
    leaves behind."""
    with tempfile.TemporaryDirectory() as directory:
        path = Path(directory) / "diagnostics.json"
        path.write_text(json.dumps(scrub_value(diagnostics()), indent=2), encoding="utf-8")
        run.hub.put(path, f"runs/{run.run}/diagnostics.json", f"run {run.run}: diagnostics")


STAGES: dict[str, Callable[[Run], None]] = {
    "diagnose": diagnose,
    "sft": sft,
    "sample": sampling,
    "rft": rft,
    "rl": rl,
    "export": export,
}


def execute(run: Run, stages: list[str], known: dict[str, Callable[[Run], None]]) -> None:
    """Run `stages` in order, the run's status written before and after each; on a failure the
    status says which stage and why, when it can be written, and the failure itself is raised
    again for the pod to end."""
    unknown = [s for s in stages if s not in known]
    if unknown:
        raise ValueError(f"unknown stages: {', '.join(unknown)}")
    for stage in stages:
        status(run, stage, "running")
        try:
            known[stage](run)
        except Exception:
            failure = traceback.format_exc()
            with contextlib.suppress(Exception):
                diagnose(run)
            with contextlib.suppress(Exception):
                status(run, stage, "failed", failure)
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
