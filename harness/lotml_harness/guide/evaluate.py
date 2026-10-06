"""The guide's offline evaluation: held-out real failures, asked through `lotml guide ask` as an
agent's tool asks, scored against the declarations the real fix changed and against the
compiler's own pointer (specs/guide-evaluation/).

    python -m lotml_harness.guide.evaluate --config <harness-guide.toml> --cpu "<cpu>" \\
        --model-file <guide.gguf> --quantization Q4_K_M [--server-pid N]
"""

import argparse
import datetime
import hashlib
import itertools
import json
import math
import re
import statistics
import subprocess
import sys
import tempfile
import time
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path

from lotml_harness import split
from lotml_harness.agent import humaneval, safe, secrets
from lotml_harness.agent.dataset import block_repairs, check_repairs
from lotml_harness.agent.report import wilson
from lotml_harness.agent.run import TRACES
from lotml_harness.experiments.phase1 import RESULTS
from lotml_harness.tasks import Task, build

MINIMUM = 97
"""Failures with a fix the evaluation needs: top-1's 95% interval then spans no more than ±10
points at a rate of one half."""
PHASE1 = RESULTS / "phase1"
ROWS = RESULTS / "guide"
REPORT = RESULTS / "guide.md"
DEADLINE = 120.0
"""Seconds one `lotml guide ask` may take: above the tool's own 75 s."""
HELD_OUT = "held-out"


class Refused(ValueError):
    """The evaluation will not run: its numbers would be worthless."""


@dataclass(frozen=True)
class Failure:
    """A real failure on a held-out problem and the fix that followed it. `blocks` are test blocks
    appended to both versions — a hidden test's, when that is what failed."""

    problem: str
    origin: str
    model: str
    kind: str
    task: str | None
    path: str
    before: str
    after: str
    blocks: str

    @property
    def failing(self) -> str:
        return _joined(self.before, self.blocks)

    @property
    def fixed(self) -> str:
        return _joined(self.after, self.blocks)


def _joined(code: str, blocks: str) -> str:
    return code if not blocks else code.rstrip("\n") + "\n\n" + blocks


def _key(failure: Failure) -> tuple[str, str]:
    return failure.problem, hashlib.sha256(failure.before.encode("utf-8")).hexdigest()


def hidden(task: Task) -> str:
    """A task set task's hidden tests as lotml test blocks."""
    cases = [(tuple(case.args), case.expected) for case in task.tests]
    return humaneval.hidden_blocks(task.name, cases, [t for _, t in task.params], task.returns)


def phase1_failures(rows: list[dict], tasks: dict[str, Task]) -> tuple[list[Failure], Counter]:
    """Each lotml answer of the phase 1 gate that failed and was followed by a passing one in the
    same conversation, on a held-out problem; a failing test's state carries the hidden blocks.
    The answers are measured with, never trained on (harness/results/NOTICE.md)."""
    found, seen, refused = [], set(), Counter()
    for row in rows:
        if row.get("language") != "lotml":
            continue
        try:
            problem = split.problem(row["task"])
        except ValueError:
            refused["unknown task"] += 1
            continue
        rounds = row.get("rounds") or []
        for failed, passed in itertools.pairwise(rounds):
            if failed["outcome"] == "pass" or passed["outcome"] != "pass":
                continue
            if split.split(problem) != HELD_OUT:
                refused["not held out"] += 1
                continue
            kind = "check" if failed["outcome"] == "does not check" else "test"
            task = tasks.get(row["task"])
            if kind == "test" and task is None:
                refused["no task"] += 1
                continue
            blocks = hidden(task) if kind == "test" else ""
            failure = Failure(
                problem, "phase1", row["model"], kind, None, "solution.lotml",
                failed["code"], passed["code"], blocks,
            )  # fmt: skip
            if _key(failure) not in seen:
                seen.add(_key(failure))
                found.append(failure)
    return found, refused


def trace_failures(directory: Path) -> tuple[list[Failure], Counter]:
    """The repairs in agent traces, by check and by test, of any model, on held-out problems."""
    found, seen, refused = [], set(), Counter()
    for path in sorted(directory.rglob("*.json")) if directory.is_dir() else []:
        if path.is_symlink():
            continue
        trace = json.loads(path.read_text(encoding="utf-8"))
        row = trace.get("row") or {}
        try:
            problem = split.problem(str(row.get("task")))
        except ValueError:
            refused["unknown task"] += 1
            continue
        for repair in check_repairs(trace) + block_repairs(trace):
            if split.split(problem) != HELD_OUT:
                refused["not held out"] += 1
                continue
            kind = "check" if repair["diagnostics"] is not None else "test"
            failure = Failure(
                problem, "agent", str(row.get("model")), kind, repair["prompt"] or None,
                repair["path"], repair["before"], repair["after"], "",
            )  # fmt: skip
            if _key(failure) not in seen:
                seen.add(_key(failure))
                found.append(failure)
    return found, refused


def guard(records: Path | None) -> str:
    """The SHA-256 of the guide's training records, once every one is read and none can leak the
    evaluation: refused when they are absent, empty or unreadable, when one has no problem or
    split, or when one's problem the split holds out (R1.3)."""
    if records is None:
        raise Refused("the guide's configuration names no training records")
    files = sorted(records.glob("*.jsonl")) if records.is_dir() else []
    if not files:
        raise Refused(f"no training records at {records}")
    digest, count = hashlib.sha256(), 0
    for file in files:
        data = file.read_bytes()
        digest.update(data)
        for line in data.decode("utf-8").splitlines():
            if not line.strip():
                continue
            try:
                meta = json.loads(line)["meta"]
                problem, bucket = meta["problem"], meta["split"]
            except (json.JSONDecodeError, KeyError, TypeError) as error:
                raise Refused(f"{file.name}: a record without its problem and split") from error
            if bucket == HELD_OUT or split.split(problem) == HELD_OUT:
                raise Refused(f"{file.name}: the guide was trained on held-out {problem}")
            count += 1
    if count == 0:
        raise Refused(f"the training records at {records} are empty")
    return digest.hexdigest()


def innermost(declared: list[dict], line: int) -> str | None:
    holding = [d for d in declared if d["lines"][0] <= line <= d["lines"][1] and d["symbol"]]
    return min(holding, key=lambda d: d["lines"][1] - d["lines"][0])["symbol"] if holding else None


def baseline_of_check(declared: list[dict], lines: list[int]) -> list[str]:
    """The compiler's pointer: the declarations holding the first three diagnostics, in order."""
    found = []
    for line in lines[:3]:
        symbol = innermost(declared, line)
        if symbol is not None and symbol not in found:
            found.append(symbol)
    return found


CALL = re.compile(r"([A-Za-z_][A-Za-z0-9_]*)\s*\(")


def baseline_of_test(declared: list[dict], expression: str) -> list[str]:
    """For a failing test, the first function of the file its block calls."""
    functions = {d["symbol"] for d in declared if d["kind"] in ("function", "method")}
    for name in CALL.findall(expression):
        if name in functions:
            return [name]
    return []


def score(truth: list[str], guide: list[str], baseline: list[str]) -> dict[str, bool]:
    """A location is right when its symbol is one the real fix changed."""
    return {
        "top1": bool(guide) and guide[0] in truth,
        "top3": any(s in truth for s in guide[:3]),
        "baseline_top1": bool(baseline) and baseline[0] in truth,
        "baseline_top3": any(s in truth for s in baseline[:3]),
    }


def _call(args: list[str], files: list[str], root: Path, env: dict | None = None) -> dict | list:
    done = safe.lotml(args, files, root, DEADLINE, env=env)
    if done is None:
        return {"guidance": None, "reason": "deadline"}
    try:
        return json.loads(done.stdout.strip().splitlines()[-1])
    except (json.JSONDecodeError, IndexError):
        return {}


def judged(failure: Failure, config: Path) -> dict:
    """One failure asked, in a scratch copy laid by the safe layer, and scored: a row of symbols,
    booleans, numbers and the tool's fixed reasons only."""
    scratch = tempfile.TemporaryDirectory(prefix="lotml-guide-eval-", ignore_cleanup_errors=True)
    with scratch as directory:
        root = Path(directory)
        safe.lay(root, {"before.lotml": failure.failing, "after.lotml": failure.fixed})
        (root / "project").mkdir()
        safe.lay(root / "project", {failure.path: failure.failing})
        args = ["guide", "ask", "--root", "."] + (["--task", failure.task] if failure.task else [])
        started = time.time()
        answer = _call(args, [failure.path], root / "project", {"LOTML_HARNESS_GUIDE": str(config)})
        seconds = time.time() - started
        diff = _call(
            ["dev", "diff", "--path", failure.path, "--json"], ["before.lotml", "after.lotml"], root
        )
        declared = _call(["dev", "outline"], ["before.lotml"], root)
        if failure.kind == "check":
            checked = _call(["check", "--json"], ["before.lotml"], root)
            lines = [
                d["location"]["line"] for d in checked.get("diagnostics", []) if "location" in d
            ]
            baseline = baseline_of_check(declared or [], lines)
        else:
            tested = _call(["test", "--json"], ["before.lotml"], root)
            rows = [t for t in tested.get("tests", []) if t.get("outcome") != "pass"]
            baseline = baseline_of_test(
                declared or [], rows[0].get("expression", "") if rows else ""
            )
    truth = [d["symbol"] for d in diff.get("declarations", []) if d.get("symbol")]
    locations = [loc["symbol"] for loc in answer.get("locations") or [] if loc.get("symbol")]
    silent = answer.get("guidance", "") is None
    return {
        "problem": failure.problem,
        "origin": failure.origin,
        "model": failure.model,
        "kind": failure.kind,
        "truth": truth,
        "guide": {
            "locations": locations,
            "kind": answer.get("kind"),
            "edit": answer.get("edit") is not None,
            "withheld": answer.get("withheld"),
            "silent": silent,
            "reason": answer.get("reason") if silent else None,
            "confidence": answer.get("confidence"),
        },
        "baseline": baseline,
        **score(truth, locations, baseline),
        "seconds": round(seconds, 2),
    }


def peak_memory(pid: int | None) -> int | None:
    """The guide server's resident memory in bytes, when its process is known."""
    if pid is None:
        return None
    if sys.platform == "win32":
        done = subprocess.run(  # noqa: S603
            ["tasklist", "/FI", f"PID eq {pid}", "/FO", "CSV", "/NH"],  # noqa: S607
            capture_output=True, text=True, check=False,
        )  # fmt: skip
        found = re.search(r'"([\d.,\s]+) K"', done.stdout)
        return int(re.sub(r"\D", "", found.group(1))) * 1024 if found else None
    status = Path(f"/proc/{pid}/status")
    found = re.search(r"VmHWM:\s+(\d+) kB", status.read_text()) if status.exists() else None
    return int(found.group(1)) * 1024 if found else None


def _rate(rows: list[dict], key: str) -> str:
    hits = sum(1 for r in rows if r[key])
    low, high = wilson(hits, len(rows))
    return (
        f"{hits}/{len(rows)} ({hits / len(rows):.0%}, 95% CI {low:.0%} to {high:.0%})"
        if rows
        else "—"
    )


def markdown(rows: list[dict], digest: str, setting: dict, day: str) -> str:
    """The committed report, built from the rows alone (R1.4, R2.4-R2.7)."""
    answered = [r for r in rows if not r["guide"]["silent"]]
    proposed = [r for r in rows if r["guide"]["edit"] or r["guide"]["withheld"]]
    withheld = Counter(r["guide"]["withheld"] for r in proposed if r["guide"]["withheld"])
    silences = Counter(r["guide"]["reason"] for r in rows if r["guide"]["silent"])
    seconds = sorted(r["seconds"] for r in rows)
    missing = max(0, MINIMUM - len(rows))
    verdict = (
        f"**No verdict:** {len(rows)} failures, {missing} short of the {MINIMUM} that bound "
        "top-1's interval to ±10 points."
        if missing
        else f"{len(rows)} failures, at least the {MINIMUM} the verdict needs."
    )
    lines = [
        "# The guide, offline",
        "",
        f"Written by `python -m lotml_harness.guide.evaluate` on {day}: held-out real",
        "failures only, asked through `lotml guide ask`, scored against the declarations the",
        "real fix changed (specs/guide-evaluation/).",
        f"Training records' SHA-256: `{digest}`.",
        "",
        verdict,
        "",
        "| | top-1 | top-3 |",
        "|---|---|---|",
        f"| guide | {_rate(rows, 'top1')} | {_rate(rows, 'top3')} |",
        f"| compiler's pointer | {_rate(rows, 'baseline_top1')} | {_rate(rows, 'baseline_top3')} |",
        "",
        f"Silent on {len(rows) - len(answered)} of {len(rows)}"
        + (f" ({', '.join(f'{k} {n}' for k, n in sorted(silences.items()))})" if silences else "")
        + f"; top-1 precision over the answers given: {_rate(answered, 'top1')}.",
        "",
        f"Edits proposed: {len(proposed)}; shown {sum(1 for r in proposed if r['guide']['edit'])}"
        + (
            f"; withheld {', '.join(f'{k} {n}' for k, n in sorted(withheld.items()))}"
            if withheld
            else ""
        )
        + ".",
        "",
    ]
    if seconds:
        p95 = seconds[min(len(seconds) - 1, math.ceil(0.95 * len(seconds)) - 1)]
        lines += [
            f"Latency: median {statistics.median(seconds):.2f} s, 95th percentile {p95:.2f} s."
        ]
    memory = setting.get("memory")
    lines += [
        f"Guide server: {setting.get('model_file')}, {setting.get('quantization')}, on "
        f"{setting.get('cpu')}; peak memory "
        + (f"{memory / 2**20:.0f} MiB." if memory else "not read."),
        "",
    ]
    for title, field in (
        ("kind of failure", "kind"),
        ("model that failed", "model"),
        ("origin", "origin"),
    ):
        lines += [
            f"| {title} | failures | guide top-1 | compiler's top-1 |",
            "|---|---:|---:|---:|",
        ]
        for value in sorted({r[field] for r in rows}):
            part = [r for r in rows if r[field] == value]
            guide = sum(r["top1"] for r in part) / len(part)
            base = sum(r["baseline_top1"] for r in part) / len(part)
            lines.append(f"| {value} | {len(part)} | {guide:.0%} | {base:.0%} |")
        lines.append("")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> None:
    from lotml_harness.guide import config as guide_config

    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--config", type=Path, required=True)
    parser.add_argument("--cpu", required=True)
    parser.add_argument("--model-file", required=True)
    parser.add_argument("--quantization", required=True)
    parser.add_argument("--server-pid", type=int)
    parser.add_argument("--workers", type=int, default=1)
    args = parser.parse_args(argv)
    config = args.config.resolve()
    digest = guard(guide_config.records(config))
    rows_found = [
        json.loads(line)
        for path in sorted(PHASE1.glob("*.jsonl"))
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    tasks = {t.id: t for t in build.load()}
    from_phase1, refused_phase1 = phase1_failures(rows_found, tasks)
    from_traces, refused_traces = trace_failures(TRACES)
    failures = from_phase1 + from_traces
    before = peak_memory(args.server_pid)
    with ThreadPoolExecutor(max_workers=args.workers) as pool:
        rows = list(pool.map(lambda f: judged(f, config), failures))
    after = peak_memory(args.server_pid)
    memory = max(m for m in (before, after, 0) if m is not None) or None
    day = datetime.date.today().isoformat()
    ROWS.mkdir(parents=True, exist_ok=True)
    lines = "".join(json.dumps(secrets.scrub_value(r)) + "\n" for r in rows)
    (ROWS / f"{day}.jsonl").write_text(lines, encoding="utf-8")
    setting = {"cpu": args.cpu, "model_file": args.model_file, "quantization": args.quantization,
               "memory": memory}  # fmt: skip
    text = secrets.scrub(markdown(rows, digest, setting, day))
    refused = refused_phase1 + refused_traces
    if refused:
        text += "Left out: " + ", ".join(f"{k} {n}" for k, n in sorted(refused.items())) + ".\n"
    REPORT.write_text(text, encoding="utf-8")
    print(text)


if __name__ == "__main__":
    main()
