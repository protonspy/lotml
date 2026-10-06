"""Translate the source datasets into the harness task set and keep what can be trusted."""

import hashlib
import json
import os
import subprocess
import sys
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from pathlib import Path

from lotml_harness import ROOT
from lotml_harness.execute import child_environment
from lotml_harness.tasks import Task, livecodebench, sources
from lotml_harness.tasks.extract import Untranslatable, from_multipl_e

HARNESS = ROOT / "harness"
TASKS = sources.CACHE / "tasks"
REPORT = HARNESS / "results" / "tasks.md"
SOURCES = ("humaneval", "mbpp", "livecodebench")
PAIRED_TASKS_NEEDED = 168
"""McNemar, two-sided at 5% with 80% power, for 10 points at 20% discordant pairs."""


@dataclass
class Report:
    read: Counter = field(default_factory=Counter)
    refused: dict[str, Counter] = field(default_factory=lambda: defaultdict(Counter))
    unchecked: Counter = field(default_factory=Counter)

    def refuse(self, source: str, reason: str) -> None:
        self.refused[source][reason.split(":")[0]] += 1


def translate(sources: dict[str, str], report: Report) -> list[Task]:
    """Each MultiPL-E file, by task id, translated; refusals counted by reason."""
    tasks = []
    for task_id, text in sources.items():
        try:
            tasks.append(from_multipl_e(task_id, text))
        except Untranslatable as error:
            report.refuse(task_id.split("/")[0], str(error))
    return tasks


def passes(task: Task, timeout: float) -> bool:
    """Whether the canonical solution passes every hidden test, run in a child process."""
    environment = child_environment()
    try:
        child = subprocess.run(  # noqa: S603
            [sys.executable, "-m", "lotml_harness.tasks.canonical"],
            input=json.dumps(task.to_json()),
            capture_output=True,
            text=True,
            timeout=timeout,
            env=environment,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return False
    if child.returncode != 0:
        return False
    try:
        return all(json.loads(child.stdout))
    except json.JSONDecodeError:
        return False


def validate(tasks: list[Task], report: Report, timeout: float = 30) -> list[Task]:
    """Tasks whose canonical solution passes; tasks without one are kept and counted."""
    checked = [t for t in tasks if t.canonical]
    for task in tasks:
        if not task.canonical:
            report.unchecked[task.source] += 1
    with ThreadPoolExecutor(max_workers=os.cpu_count() or 4) as pool:
        verdicts = iter(list(pool.map(lambda t: passes(t, timeout), checked)))
    kept = []
    for task in tasks:
        if task.canonical and not next(verdicts):
            report.refuse(task.source, "canonical fails")
        else:
            kept.append(task)
    return kept


def from_livecodebench(records: list[dict], report: Report) -> list[Task]:
    tasks = []
    for record in records:
        try:
            tasks.append(livecodebench.from_record(record))
        except Untranslatable as error:
            report.refuse("livecodebench", str(error))
    return tasks


def build(timeout: float = 30) -> tuple[list[Task], Report]:
    """Fetch every source, translate it, and keep the tasks that can be trusted."""
    report = Report()
    files = {}
    for source in ("humaneval", "mbpp"):
        files |= sources.multipl_e(source)
        report.read[source] = sum(1 for k in files if k.startswith(source))
    tasks = translate(files, report)
    canonical = sources.mbpp_canonical()
    for task in tasks:
        if task.source == "mbpp":
            task.canonical = canonical.get(task.id, "")
    records = sources.livecodebench()
    report.read["livecodebench"] = len(records)
    tasks += from_livecodebench(records, report)
    return validate(tasks, report, timeout), report


def write(tasks: list[Task], directory: Path = TASKS) -> dict[str, str]:
    """One JSONL file per source; returns each file's SHA-256."""
    directory.mkdir(parents=True, exist_ok=True)
    digests = {}
    for source in SOURCES:
        lines = [
            json.dumps(t.to_json(), ensure_ascii=False, sort_keys=True)
            for t in tasks
            if t.source == source
        ]
        data = ("\n".join(lines) + "\n").encode()
        (directory / f"{source}.jsonl").write_bytes(data)
        digests[source] = hashlib.sha256(data).hexdigest()
    return digests


def load(directory: Path = TASKS, only: tuple[str, ...] = SOURCES) -> list[Task]:
    """The task set the build wrote, in source order."""
    tasks = []
    for source in only:
        path = directory / f"{source}.jsonl"
        if not path.exists():
            raise FileNotFoundError(f"{path}: run `python -m lotml_harness.tasks.build`")
        with path.open(encoding="utf-8") as lines:
            tasks += [Task.from_json(json.loads(line)) for line in lines if line.strip()]
    return tasks


def markdown(tasks: list[Task], report: Report, digests: dict[str, str]) -> str:
    kept = Counter(t.source for t in tasks)
    cases = Counter()
    for task in tasks:
        cases[task.source] += len(task.tests)
    lines = [
        "# Task set",
        "",
        "Generated by `python -m lotml_harness.tasks.build` from MultiPL-E's typed Python",
        f"originals at `{sources.MULTIPL_E[:12]}` and LiveCodeBench release v6",
        f"(`test6.jsonl` at `{sources.LIVECODEBENCH[:12]}`). The tasks themselves are",
        "written to the git-ignored `harness/cache/tasks/`; the digests below identify them.",
        "",
        "| source | read | kept | hidden tests | checked by a canonical solution |",
        "| --- | ---: | ---: | ---: | ---: |",
    ]
    for source in SOURCES:
        checked = kept[source] - report.unchecked[source]
        lines.append(
            f"| {source} | {report.read[source]} | {kept[source]} | {cases[source]} | {checked} |"
        )
    total = sum(kept.values())
    lines += [
        f"| total | {sum(report.read.values())} | {total} | {sum(cases.values())} "
        f"| {total - sum(report.unchecked.values())} |",
        "",
        f"Every task can be posed in either variant, so a comparison between them has up to "
        f"{total} paired tasks against the {PAIRED_TASKS_NEEDED} the sample size requires "
        f"({'met' if total >= PAIRED_TASKS_NEEDED else 'not met'}).",
        "",
        "## Refused, by reason",
        "",
        "| source | reason | tasks |",
        "| --- | --- | ---: |",
    ]
    for source in SOURCES:
        for reason, count in sorted(report.refused[source].items(), key=lambda r: -r[1]):
            lines.append(f"| {source} | {reason} | {count} |")
    lines += ["", "## Digests", ""]
    lines += [f"- `{source}.jsonl`: `{digests[source]}`" for source in SOURCES]
    return "\n".join(lines) + "\n"


def main() -> None:
    tasks, report = build()
    digests = write(tasks)
    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_text(markdown(tasks, report, digests), encoding="utf-8")
    print(REPORT.read_text(encoding="utf-8"))


if __name__ == "__main__":
    main()
