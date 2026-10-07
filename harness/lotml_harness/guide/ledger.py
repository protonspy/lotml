"""The RunPod ledger: every pod a run created, what it cost, and the cap a new run is held to
(specs/training-pipeline/ R1.2, R1.3).

Amounts are `Decimal` USD, written as strings. A pod still open counts at its full deadline, so a
crash between creating a pod and recording its end can only overstate the spend, never hide it.
The ledger that holds the cap lives in the user's home, outside every checkout, so a branch, a
worktree or a reset cannot lower what was spent; the committed copy under `harness/results/` is
its record, and the spend counts the rows of both.
"""

import contextlib
import json
import math
import os
import time
from collections.abc import Iterator
from datetime import datetime
from decimal import ROUND_UP, Decimal
from pathlib import Path

from lotml_harness.experiments.phase1 import RESULTS

LEDGER = Path.home() / ".lotml" / "runpod.jsonl"
COMMITTED = RESULTS / "runpod.jsonl"
REPORT = RESULTS / "runpod.md"
LOCK_WAIT = 30.0
"""Seconds a change waits for another run's change to the ledger."""
CAP = Decimal("25.00")
"""USD the user approved for RunPod on 2026-10-07."""
CENT = Decimal("0.0001")


class CapExceeded(RuntimeError):
    """A run whose estimate would take the spend past the cap."""


def estimate(hourly: Decimal, hours: float) -> Decimal:
    """What a run costs if it lasts until its deadline."""
    return (hourly * Decimal(str(hours))).quantize(CENT, rounding=ROUND_UP)


def _read(path: Path | None) -> list[dict]:
    if path is None or not path.exists():
        return []
    text = path.read_text(encoding="utf-8")
    return [json.loads(line) for line in text.splitlines() if line.strip()]


class Ledger:
    """The rows of `path`, one per pod, rewritten whole on every change under a lock; `mirror` is
    the committed copy, whose rows the spend also counts."""

    def __init__(self, path: Path = LEDGER, mirror: Path | None = COMMITTED) -> None:
        self.path, self.mirror = path, mirror

    def rows(self) -> list[dict]:
        return _read(self.path)

    def every(self) -> list[dict]:
        """This ledger's rows and the committed copy's, one per pod, this ledger's winning."""
        found = {row["pod"]: row for row in _read(self.mirror)}
        found |= {row["pod"]: row for row in self.rows()}
        return list(found.values())

    @contextlib.contextmanager
    def _locked(self) -> Iterator[None]:
        """Hold the ledger's lock file while a change reads and rewrites the ledger."""
        self.path.parent.mkdir(parents=True, exist_ok=True)
        lock = self.path.with_suffix(".lock")
        waited = 0.0
        while True:
            try:
                handle = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY)
                break
            except FileExistsError:
                if waited >= LOCK_WAIT:
                    raise TimeoutError(f"{lock} is held: another run is changing it") from None
                time.sleep(0.1)
                waited += 0.1
        try:
            yield
        finally:
            os.close(handle)
            lock.unlink(missing_ok=True)

    def _write(self, rows: list[dict]) -> None:
        lines = "".join(json.dumps(row) + "\n" for row in rows)
        self.path.write_text(lines, encoding="utf-8", newline="")

    def commit(self) -> None:
        """Write every row to the committed copy, the ledger's record in the repository."""
        if self.mirror is None:
            return
        self.mirror.parent.mkdir(parents=True, exist_ok=True)
        lines = "".join(json.dumps(row) + "\n" for row in self.every())
        self.mirror.write_text(lines, encoding="utf-8", newline="")

    def open(
        self,
        run: str,
        pod: str,
        stages: list[str],
        gpu: str,
        cloud: str,
        hourly: Decimal,
        hours: float,
        started: datetime,
    ) -> dict:
        """Record a pod as created, before anything else can fail, with no end yet."""
        row = {
            "run": run, "pod": pod, "stages": stages, "gpu": gpu, "cloud": cloud,
            "hourly": str(hourly), "hours": hours, "started": started.isoformat(),
            "ended": None, "minutes": None, "cost": None,
        }  # fmt: skip
        with self._locked():
            self._write([*self.rows(), row])
        return row

    def close(self, pod: str, ended: datetime) -> dict:
        """Record a pod's confirmed end and its cost: its minutes, rounded up, at its price.
        KeyError for a pod not open in the ledger."""
        with self._locked():
            rows = self.rows()
            found = next((r for r in rows if r["pod"] == pod and r["ended"] is None), None)
            if found is None:
                raise KeyError(f"no open pod {pod} in the ledger")
            seconds = (ended - datetime.fromisoformat(found["started"])).total_seconds()
            minutes = max(0, math.ceil(seconds / 60))
            cost = (Decimal(found["hourly"]) * minutes / 60).quantize(CENT, rounding=ROUND_UP)
            found |= {"ended": ended.isoformat(), "minutes": minutes, "cost": str(cost)}
            self._write(rows)
        return found

    def spent(self) -> Decimal:
        """Closed pods at their cost, open ones at their full deadline, over this ledger's rows and
        the committed copy's."""
        return sum(
            (
                Decimal(r["cost"])
                if r["ended"] is not None
                else estimate(Decimal(r["hourly"]), r["hours"])
                for r in self.every()
            ),
            Decimal(0),
        )

    def check(self, cap: Decimal, estimated: Decimal) -> None:
        """CapExceeded when `estimated` added to what is spent would pass `cap`."""
        spent = self.spent()
        if spent + estimated > cap:
            raise CapExceeded(f"{spent:.2f} spent + {estimated:.2f} estimated > {cap:.2f} USD cap")

    def markdown(self, cap: Decimal) -> str:
        """The committed report: every pod, and the total against the cap."""
        lines = [
            "# RunPod ledger",
            "",
            "Every pod the guide's training pipeline created (specs/training-pipeline/),",
            "written by `python -m lotml_harness.guide.pipeline`.",
            "A pod still open counts at its deadline.",
            "",
            "| run | pod | stages | GPU | cloud | USD/h | started | minutes | USD |",
            "|---|---|---|---|---|---:|---|---:|---:|",
        ]
        for r in self.every():
            if r["ended"] is None:
                spent = estimate(Decimal(r["hourly"]), r["hours"])
                cost = f"open, counted at its {r['hours']} h deadline: {spent:.2f}"
                minutes = ""
            else:
                cost, minutes = f"{Decimal(r['cost']):.2f}", str(r["minutes"])
            lines.append(
                f"| {r['run']} | {r['pod']} | {', '.join(r['stages'])} | {r['gpu']} | {r['cloud']} "
                f"| {Decimal(r['hourly']):.2f} | {r['started']} | {minutes} | {cost} |"
            )
        lines += ["", f"Spent {self.spent():.2f} of {cap:.2f} USD.", ""]
        return "\n".join(lines)
