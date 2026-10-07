"""The guide's training pipeline from this machine: records checked and uploaded, the run priced
against the cap, a pod created and watched, then terminated whatever happened, the ledger and the
run's report committed (specs/training-pipeline/).

    uv run --group guide python -m lotml_harness.guide.pipeline run --stages sft,rl,export \\
        --hub <user>/<repo> --gpu "NVIDIA GeForce RTX 4090" --hours 3 [--from rl=<run>] [--dry-run]
    uv run --group guide python -m lotml_harness.guide.pipeline reconcile

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
TERMINATION = 300
"""Seconds `runpod.terminate` waits for a pod to go."""
MAX_HOURS = 12.0
"""The longest deadline a run may ask for."""
PREFIX = "lotml-guide-"
"""Every pod the pipeline creates is named with it, and `reconcile` looks for it."""


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


@dataclass
class Ready:
    """A run checked, priced and uploaded, a pod's request built for each GPU it may run on, in
    the order they are tried; `gpu` and `hourly` are the one RunPod had a card for, once created."""

    run: str
    options: list[tuple[str, Decimal, dict]]
    hours: float
    """The hours the pod can bill at most: the deadline, its grace, and the termination."""
    gpu: str = ""
    hourly: Decimal = Decimal(0)


def billed(hours: float) -> float:
    """The hours a pod with this deadline can bill at most: the watcher waits `GRACE` past it,
    and a termination may take `runpod.terminate`'s wait."""
    return hours + (GRACE + TERMINATION) / 3600


def prepare(
    plan: Plan,
    store: Hub,
    ledger: ledgers.Ledger,
    commit: str,
    dry: bool = False,
    on_github: Callable[[str], bool] | None = None,
) -> Ready | None:
    """Everything before a pod exists: the run checked, priced and uploaded, its request built —
    or, dry, that request printed with the token hidden. Refused when the stages are unknown, the
    records unusable, the commit not pushed, or the estimate past the cap (R1.2, R1.6, R2.5,
    R3.1)."""
    unknown = [s for s in plan.stages if s not in STAGES]
    if not plan.stages or unknown:
        raise Refused(f"unknown stages: {', '.join(unknown) or 'none named'}")
    if not 0 < plan.hours <= MAX_HOURS:
        raise Refused(f"a run's deadline is more than 0 and at most {MAX_HOURS:g} hours")
    records = plan.records or latest(RECORDS)
    if records is None:
        raise Refused("no guidance records: build them with guide.seeded and guide.records")
    checked(records)
    if not (on_github or pushed)(commit):
        raise Refused(f"{commit[:12]} is on no remote branch: push it, the pod clones it")
    gpus = [g.strip() for g in plan.gpu.split(",") if g.strip()]
    prices = {gpu: Decimal(str(runpod.price(gpu, plan.cloud))) for gpu in gpus}
    hourly = max(prices.values())
    hours = billed(plan.hours)
    estimated = ledgers.estimate(hourly, hours)
    ledger.check(plan.cap, estimated)
    run = now().strftime("%Y%m%d-%H%M%S")
    seconds = int(plan.hours * 3600)
    env = pod.environment(run, commit, plan.stages, seconds, plan.hub, "", plan.inputs)
    name = f"{PREFIX}{run}"
    if dry:
        shown = env | {"HF_TOKEN": "<redacted>"}
        request = runpod.pod_request(
            name, pod.IMAGE, gpus[0], plan.cloud, pod.DISK, shown, pod.WRAPPER, pod.CUDA
        )
        printed = {
            "estimate_usd": str(estimated),
            "hourly_usd": {gpu: str(price) for gpu, price in prices.items()},
            "request": request,
        }
        print(json.dumps(printed, indent=2))
        return None
    digest = store.put_records(records)
    store.put_lineage(run, digest, plan.inputs, commit)
    env["LOTML_RECORDS"] = digest
    options = [
        (
            gpu,
            prices[gpu],
            runpod.pod_request(
                name, pod.IMAGE, gpu, plan.cloud, pod.DISK, env, pod.WRAPPER, pod.CUDA
            ),
        )
        for gpu in gpus
    ]
    return Ready(run, options, hours)


UNAVAILABLE = "no longer any instances available"
"""RunPod's word for a GPU it has no card of right now, which the next one in the list may have."""


def create(ready: Ready) -> str:
    """Create the pod on the first GPU RunPod has a card for, in the order given; its id, with
    `ready.gpu` and `ready.hourly` set. A creation RunPod may have made though its answer never
    came back is looked for by the pod's name, unique to the run, so it is terminated like any
    other; a GPU with no card free, or one RunPod failed to create on its side, gives way to the
    next."""
    failure: runpod.RunPodError | None = None
    for gpu, hourly, request in ready.options:
        ready.gpu, ready.hourly = gpu, hourly
        try:
            return runpod.create(request)["id"]
        except runpod.RunPodError as error:
            found = None if UNAVAILABLE in str(error) else runpod.named(request["name"])
            if found is not None:
                return found
            if UNAVAILABLE in str(error) or str(error).startswith("RunPod answered 5"):
                failure = error
                continue
            raise
    raise failure or runpod.RunPodError("no GPU was named")


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
    its grace pass; the last status seen. A look that fails — the network, the repository — is
    said and looked again at, the deadline still bounding it."""
    seen: dict | None = None
    while True:
        try:
            found = status(store, run)
            gone = runpod.status(pod_id) is None
        except Exception as error:  # noqa: BLE001 - a failed look must not end the watch early
            say(f"{run}: could not look ({type(error).__name__}), looking again")
            found, gone = seen, False
        if found is not None and found != seen:
            say(f"{run}: {found['stage']} {found['state']}")
            seen = found
        if seen is not None and seen["state"] in ("done", "failed"):
            return seen
        if gone:
            say(f"{run}: pod {pod_id} is gone")
            return seen
        if now() > deadline + datetime.timedelta(seconds=GRACE):
            say(f"{run}: past its deadline")
            return seen
        (sleep or time.sleep)(POLL)


def finish(
    store: Hub,
    ledger: ledgers.Ledger,
    ready: Ready,
    pod_id: str,
    created: datetime.datetime,
    plan: Plan,
    seen: dict | None,
) -> Path:
    """Terminate the pod and confirm it gone, close its ledger row — written here if it never
    was — and commit the ledger's report and the run's (R1.3, R1.4, R5.1). A pod that would not
    go keeps its row open, counted at its full deadline until `reconcile`, and the failure is
    raised after the ledger's report is written."""
    if not any(r["pod"] == pod_id for r in ledger.rows()):
        ledger.open(
            ready.run,
            pod_id,
            plan.stages,
            ready.gpu,
            plan.cloud,
            ready.hourly,
            ready.hours,
            created,
        )
    try:
        runpod.terminate(pod_id)
    except runpod.RunPodError:
        ledger.commit()
        ledgers.REPORT.write_text(ledger.markdown(plan.cap), encoding="utf-8")
        raise
    row = ledger.close(pod_id, now())
    ledger.commit()
    ledgers.REPORT.write_text(ledger.markdown(plan.cap), encoding="utf-8")
    found = fetched(store, ready.run)
    REPORTS.mkdir(parents=True, exist_ok=True)
    path = REPORTS / f"{ready.run}.md"
    path.write_text(
        anonymised(scrub(markdown(ready.run, plan, row, seen, found))), encoding="utf-8"
    )
    return path


def fetched(store: Hub, run: str) -> dict:
    """The run's reports from the repository, by name, each when it exists: run.json, the
    supervised and reinforcement-learning reports, and the export's report.json."""
    found: dict = {}
    local = RUNS / run
    for name, remote in (
        ("run", f"runs/{run}/run.json"),
        ("sft", f"runs/{run}/sft/train.json"),
        ("sample", f"runs/{run}/sample/sample.json"),
        ("rft", f"runs/{run}/rft/rft.json"),
        ("rl", f"runs/{run}/rl/rl.json"),
        ("rl-random", f"runs/{run}/rl-random/rl.json"),
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
    for name in ("sft", "rft", "rl", "rl-random"):
        if name in found:
            report = found[name]
            settings = ", ".join(f"{k} {v}" for k, v in report.get("settings", {}).items())
            lines.append(f"- {name}: {report.get('seconds')} s on {report.get('gpu')}; {settings}.")
    if "sample" in found:
        sampled = found["sample"]
        lines.append(
            f"- Sampled: {sampled.get('records')} train records, mean reward "
            f"{sampled.get('mean', 0):.3f}, {sampled.get('always')} always solved, "
            f"{sampled.get('never')} never, {sampled.get('unjudged', 0)} answers unjudged."
        )
    if "rft" in found:
        rft = found["rft"]
        lines.append(
            f"- Rejection sampling: {rft.get('answers')} passing answers and {rft.get('targets')} "
            f"targets over {rft.get('records')} records."
        )
    for name in ("rl", "rl-random"):
        if name in found:
            rl = found[name]
            alike = rl.get("alike")
            lines.append(
                f"- {name}: a pool of {rl.get('pool')} of {rl.get('sampled')} sampled records; "
                f"reward {rl.get('reward_first')} at the start, "
                f"{rl.get('reward_last')} at the end; "
                + (f"{_share(alike)} of groups scored alike; " if alike is not None else "")
                + f"validation rewards {rl.get('eval_rewards')}; best at {rl.get('best')}; "
                f"{rl.get('unjudged', 0)} answers unjudged."
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
        passed = {name: m["pass"] for name, m in export["models"].items() if "pass" in m}
        if passed:
            lines += [
                "",
                "Sampled eight times at temperature 1.0, judged by the compiler:",
                "",
                "| model | records | location@1 | answer@1 | answer@4 | answer@8 |",
                "|---|---:|---:|---:|---:|---:|",
            ]
            for name, p in passed.items():
                cells = " | ".join(
                    _share(p[k]) if k in p else "—"
                    for k in ("location@1", "answer@1", "answer@4", "answer@8")
                )
                lines.append(f"| {name} | {p.get('records')} | {cells} |")
        lines += [
            "",
            f"Threshold for {export['final']}: {export['threshold']:.4f}, at which "
            f"{_share(export['shown'])} of the records are answered with first-location precision "
            f"{_share(export['precision'])} (target {_share(export['target'])}).",
        ]
    return "\n".join(lines) + "\n"


def run(plan: Plan, store: Hub, ledger: ledgers.Ledger, dry: bool = False) -> Path | None:
    """One run end to end; the report's path. From the moment a pod may exist, whatever happens,
    it is terminated and recorded."""
    ready = prepare(plan, store, ledger, head(), dry)
    if ready is None:
        return None
    pod_id, created, seen = None, now(), None
    try:
        pod_id = create(ready)
        ledger.open(
            ready.run,
            pod_id,
            plan.stages,
            ready.gpu,
            plan.cloud,
            ready.hourly,
            ready.hours,
            created,
        )
        seen = watch(store, ready.run, pod_id, created + datetime.timedelta(hours=plan.hours))
    finally:
        if pod_id is not None:
            path = finish(store, ledger, ready, pod_id, created, plan, seen)
    return path


def reconcile(ledger: ledgers.Ledger) -> None:
    """Close every row a crash left open: a pod still there is terminated and closed now; a pod
    gone is closed at its deadline, or now if that is sooner, which can only overstate what it
    cost. Then terminate and record any pod the pipeline named that the ledger never saw."""
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
    recorded = {row["pod"] for row in ledger.every()}
    for found in runpod.pods():
        if not str(found.get("name", "")).startswith(PREFIX) or found["id"] in recorded:
            continue
        runpod.terminate(found["id"])
        started = datetime.datetime.fromisoformat(
            str(found.get("createdAt") or now().isoformat()).replace("Z", "+00:00")
        )
        gpu = (found.get("gpu") or {}).get("id", "unknown")
        hourly = Decimal(str(found.get("cost") or 0))
        run_id = str(found["name"]).removeprefix(PREFIX)
        ledger.open(run_id, found["id"], [], gpu, str(found.get("cloud", "")), hourly, 0, started)
        ledger.close(found["id"], now())
    ledger.commit()
    ledgers.REPORT.write_text(ledger.markdown(ledgers.CAP), encoding="utf-8")


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest="command", required=True)
    go = sub.add_parser("run")
    go.add_argument("--stages", required=True, help="comma separated: sft, rl, export")
    go.add_argument("--hub", required=True, help="the private repository, <user>/<name>")
    go.add_argument(
        "--gpu",
        default="NVIDIA GeForce RTX 4090,NVIDIA RTX A6000,NVIDIA GeForce RTX 3090,NVIDIA RTX A5000",
        help="GPU types, comma separated, tried in order until RunPod has a card",
    )
    go.add_argument("--cloud", default="COMMUNITY", choices=("COMMUNITY", "SECURE"))
    go.add_argument("--hours", type=float, required=True, help="the run's deadline")
    go.add_argument("--from", dest="inputs", action="append", default=[], help="stage=run")
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
        records=args.records,
    )  # fmt: skip
    path = run(plan, Hub(args.hub), ledger, args.dry_run)
    if path is not None:
        print(path.read_text(encoding="utf-8"))


if __name__ == "__main__":
    main()
