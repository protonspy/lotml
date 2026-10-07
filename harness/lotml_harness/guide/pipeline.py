"""The guide's training pipeline from this machine: records checked and uploaded, the run priced
against the cap, a pod created and watched, then terminated whatever happened, the ledger and the
run's report committed (specs/training-pipeline/).

    python -m lotml_harness.guide.pipeline run --stages sft,rl,export --hub <user>/<repo> \\
        --gpu "NVIDIA GeForce RTX 4090" --hours 3 [--from rl=<run>] [--dry-run]
    python -m lotml_harness.guide.pipeline reconcile

Keys come from the environment: `RUNPOD_API_KEY` for RunPod, `HF_TOKEN` for the repository.
"""

import argparse
import datetime
import json
import subprocess
import tempfile
import time
from collections.abc import Callable
from dataclasses import dataclass, field
from decimal import Decimal
from pathlib import Path

from lotml_harness import ROOT
from lotml_harness.agent.secrets import anonymised, scrub
from lotml_harness.experiments.phase1 import RESULTS
from lotml_harness.guide import ledger as ledgers
from lotml_harness.guide import pod, runpod, train
from lotml_harness.guide.hub import Hub
from lotml_harness.guide.records import RECORDS, latest
from lotml_harness.guide.stages import STAGES

REPORTS = RESULTS / "training"
RUNS = ROOT / "harness" / "cache" / "guide" / "runs"
POLL = 60.0
"""Seconds between looks at a running pod."""
GRACE = 600
"""Seconds past the deadline the watcher waits for the pod to remove itself before it does."""


class Refused(RuntimeError):
    """A run the pipeline will not start."""


def now() -> datetime.datetime:
    return datetime.datetime.now(datetime.UTC)


def checked(records: Path) -> None:
    """Every record within its split, none held out (R2.5), and each with the state the reward
    judges against."""
    for bucket in ("train", "validation"):
        found = train.load(records, bucket)
        if not found:
            raise Refused(f"{records} holds no {bucket} records")
        if any("state" not in r for r in found):
            raise Refused(f"{records} predates the records' state: build them again")


def pushed(commit: str) -> bool:
    """Whether `commit` is on a remote branch, where the pod's clone can find it."""
    done = subprocess.run(  # noqa: S603
        ["git", "branch", "-r", "--contains", commit],  # noqa: S607
        capture_output=True, text=True, check=False, cwd=ROOT,
    )  # fmt: skip
    return done.returncode == 0 and bool(done.stdout.strip())


def head() -> str:
    command = ["git", "rev-parse", "HEAD"]
    done = subprocess.run(command, capture_output=True, text=True, check=True, cwd=ROOT)  # noqa: S603
    return done.stdout.strip()


@dataclass
class Plan:
    """One run as asked for on the command line."""

    stages: list[str]
    hub: str
    gpu: str
    hours: float
    cloud: str = "COMMUNITY"
    inputs: dict[str, str] = field(default_factory=dict)
    cap: Decimal = ledgers.CAP
    records: Path | None = None


def start(
    plan: Plan,
    store: Hub,
    ledger: ledgers.Ledger,
    commit: str,
    dry: bool = False,
    on_github: Callable[[str], bool] | None = None,
) -> tuple[str, dict] | None:
    """Everything before the pod runs: the run's id, and its pod as created — or, dry, the request
    it would send with the token hidden, printed. Refused when the stages are unknown, the records
    unusable, the commit not pushed, or the estimate past the cap (R1.2, R1.6, R2.5, R3.1)."""
    unknown = [s for s in plan.stages if s not in STAGES]
    if not plan.stages or unknown:
        raise Refused(f"unknown stages: {', '.join(unknown) or 'none named'}")
    records = plan.records or latest(RECORDS)
    if records is None:
        raise Refused("no guidance records: build them with guide.seeded and guide.records")
    checked(records)
    if not (on_github or pushed)(commit):
        raise Refused(f"{commit[:12]} is on no remote branch: push it, the pod clones it")
    hourly = Decimal(str(runpod.price(plan.gpu, plan.cloud)))
    estimated = ledgers.estimate(hourly, plan.hours)
    ledger.check(plan.cap, estimated)
    run = now().strftime("%Y%m%d-%H%M%S")
    seconds = int(plan.hours * 3600)
    env = pod.environment(run, commit, plan.stages, seconds, plan.hub, "", plan.inputs)
    if dry:
        shown = env | {"HF_TOKEN": "<redacted>"}
        request = runpod.pod_request(
            f"lotml-guide-{run}", pod.IMAGE, plan.gpu, plan.cloud, pod.DISK, shown, pod.WRAPPER
        )
        print(
            json.dumps(
                {"estimate_usd": str(estimated), "hourly_usd": str(hourly), "request": request},
                indent=2,
            )
        )
        return None
    digest = store.put_records(records)
    store.put_lineage(run, digest, plan.inputs, commit)
    env["LOTML_RECORDS"] = digest
    request = runpod.pod_request(
        f"lotml-guide-{run}", pod.IMAGE, plan.gpu, plan.cloud, pod.DISK, env, pod.WRAPPER
    )
    created = runpod.create(request)
    ledger.open(run, created["id"], plan.stages, plan.gpu, plan.cloud, hourly, plan.hours, now())
    return run, created


def status(store: Hub, run: str) -> dict | None:
    """The run's `status.json`, or None before the pod wrote one."""
    if not store.files(f"runs/{run}/status.json"):
        return None
    with tempfile.TemporaryDirectory() as directory:
        path = store.get(f"runs/{run}/status.json", Path(directory))
        return json.loads(path.read_text(encoding="utf-8"))


def watch(
    store: Hub,
    run: str,
    pod_id: str,
    deadline: datetime.datetime,
    sleep: Callable[[float], None] | None = None,
    say: Callable[[str], None] = print,
) -> dict | None:
    """Look at the run until it says it is done or failed, its pod is gone, or the deadline and
    its grace pass; the last status seen."""
    seen: dict | None = None
    while True:
        found = status(store, run)
        if found is not None and found != seen:
            say(f"{run}: {found['stage']} {found['state']}")
            seen = found
        if seen is not None and seen["state"] in ("done", "failed"):
            return seen
        if runpod.status(pod_id) is None:
            say(f"{run}: pod {pod_id} is gone")
            return seen
        if now() > deadline + datetime.timedelta(seconds=GRACE):
            say(f"{run}: past its deadline")
            return seen
        (sleep or time.sleep)(POLL)


def finish(
    store: Hub, ledger: ledgers.Ledger, run: str, pod_id: str, plan: Plan, seen: dict | None
) -> Path:
    """Terminate the pod, confirm it gone, close its ledger row, and commit the ledger's report and
    the run's (R1.3, R1.4, R5.1)."""
    runpod.terminate(pod_id)
    row = ledger.close(pod_id, now())
    ledgers.REPORT.write_text(ledger.markdown(plan.cap), encoding="utf-8")
    found = fetched(store, run)
    REPORTS.mkdir(parents=True, exist_ok=True)
    path = REPORTS / f"{run}.md"
    path.write_text(anonymised(scrub(markdown(run, plan, row, seen, found))), encoding="utf-8")
    return path


def fetched(store: Hub, run: str) -> dict:
    """The run's reports from the repository, by name, each when it exists: run.json, the
    supervised and reinforcement-learning reports, and the export's report.json."""
    found: dict = {}
    local = RUNS / run
    for name, remote in (
        ("run", f"runs/{run}/run.json"),
        ("sft", f"runs/{run}/sft/train.json"),
        ("rl", f"runs/{run}/rl/rl.json"),
        ("export", f"runs/{run}/report.json"),
    ):
        if store.files(remote):
            found[name] = json.loads(store.get(remote, local).read_text(encoding="utf-8"))
    return found


def _share(value: float) -> str:
    return f"{value:.1%}"


def markdown(run: str, plan: Plan, row: dict, seen: dict | None, found: dict) -> str:
    """The committed report of one run (R5.1)."""
    lineage = found.get("run", {})
    lines = [
        f"# Training run {run}",
        "",
        "The guide's training pipeline on RunPod (specs/training-pipeline/), written by",
        "`python -m lotml_harness.guide.pipeline`.",
        "",
        f"- Stages: {', '.join(plan.stages)}; inputs: {lineage.get('inputs') or 'this run'}.",
        f"- Records: `{lineage.get('records', '?')}`; commit `{lineage.get('commit', '?')}`.",
        f"- Pod: {row['gpu']} on {row['cloud']} at {Decimal(row['hourly']):.2f} USD/h, "
        f"{row['minutes']} minutes, {Decimal(row['cost']):.2f} USD.",
        f"- Ended: {(seen or {}).get('state', 'without a status')}"
        + (f" in {seen['stage']}" if seen and seen.get("stage") else "")
        + ".",
    ]
    for name in ("sft", "rl"):
        if name in found:
            report = found[name]
            settings = ", ".join(f"{k} {v}" for k, v in report.get("settings", {}).items())
            lines.append(f"- {name}: {report.get('seconds')} s on {report.get('gpu')}; {settings}.")
    if "rl" in found:
        rl = found["rl"]
        lines.append(
            f"- Reward: {rl.get('reward_first')} at the start, {rl.get('reward_last')} at the end; "
            f"{rl.get('unjudged', 0)} answers the judge could not score."
        )
    export = found.get("export")
    if export:
        lines += [
            "",
            "On the validation split, asked through `lotml guide ask`:",
            "",
            "| model | records | within the schema | top-1 | top-3 | edits that pass | silent |",
            "|---|---:|---:|---:|---:|---:|---|",
        ]
        for name, m in export["models"].items():
            silent = ", ".join(f"{k} {v}" for k, v in sorted(m["silent"].items())) or "none"
            lines.append(
                f"| {name} | {m['records']} | {_share(m['within_schema'])} | {_share(m['top1'])} "
                f"| {_share(m['top3'])} | {_share(m['edits_pass'])} | {silent} |"
            )
        lines += [
            "",
            f"Threshold for {export['final']}: {export['threshold']:.4f}, at which "
            f"{_share(export['shown'])} of the records are answered with first-location precision "
            f"{_share(export['precision'])} (target {_share(export['target'])}).",
        ]
    return "\n".join(lines) + "\n"


def run(plan: Plan, store: Hub, ledger: ledgers.Ledger, dry: bool = False) -> Path | None:
    """One run end to end; the report's path. The pod is terminated whatever happens once it
    exists."""
    started = start(plan, store, ledger, head(), dry)
    if started is None:
        return None
    run_id, created = started
    seen = None
    try:
        deadline = now() + datetime.timedelta(hours=plan.hours)
        seen = watch(store, run_id, created["id"], deadline)
    finally:
        path = finish(store, ledger, run_id, created["id"], plan, seen)
    return path


def reconcile(ledger: ledgers.Ledger) -> None:
    """Close every row a crash left open: a pod still there is terminated and closed now; a pod
    gone is closed at its deadline, or now if that is sooner, which can only overstate what it
    cost."""
    for row in ledger.rows():
        if row["ended"] is not None:
            continue
        if runpod.status(row["pod"]) is not None:
            runpod.terminate(row["pod"])
            ended = now()
        else:
            deadline = datetime.datetime.fromisoformat(row["started"]) + datetime.timedelta(
                hours=row["hours"]
            )
            ended = min(deadline, now())
        ledger.close(row["pod"], ended)
    ledgers.REPORT.write_text(ledger.markdown(ledgers.CAP), encoding="utf-8")


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    go = sub.add_parser("run")
    go.add_argument("--stages", required=True, help="comma separated: sft, rl, export")
    go.add_argument("--hub", required=True, help="the private repository, <user>/<name>")
    go.add_argument("--gpu", default="NVIDIA GeForce RTX 4090")
    go.add_argument("--cloud", default="COMMUNITY", choices=("COMMUNITY", "SECURE"))
    go.add_argument("--hours", type=float, required=True, help="the run's deadline")
    go.add_argument("--from", dest="inputs", action="append", default=[], help="stage=run")
    go.add_argument("--cap", type=Decimal, default=ledgers.CAP)
    go.add_argument("--records", type=Path)
    go.add_argument("--dry-run", action="store_true")
    sub.add_parser("reconcile")
    args = parser.parse_args(argv)
    ledger = ledgers.Ledger()
    if args.command == "reconcile":
        reconcile(ledger)
        return
    plan = Plan(
        stages=[s for s in args.stages.split(",") if s], hub=args.hub, gpu=args.gpu,
        hours=args.hours, cloud=args.cloud, inputs=dict(i.split("=", 1) for i in args.inputs),
        cap=args.cap, records=args.records,
    )  # fmt: skip
    path = run(plan, Hub(args.hub), ledger, args.dry_run)
    if path is not None:
        print(path.read_text(encoding="utf-8"))


if __name__ == "__main__":
    main()
