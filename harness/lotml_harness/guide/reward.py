"""The reward of the guide's reinforcement learning: each answer judged by the compiler as the
`guide` tool's gate would judge it, then scored on where it points and whether its edit fixes the
file (specs/training-pipeline/ R4).

A copy of the failing body, the habit the supervised guide showed on real failures, fails check
and earns nothing for its edit, while a right location still earns half.
"""

import json
import tempfile
import threading
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path

from lotml_harness.agent import safe

DEADLINE = 20.0
"""Seconds one judgment may take, the failing block's run included."""
PASSES = "passes"
FILE = "file.lot"


@dataclass(frozen=True)
class Judged:
    """The judge's verdict: within the schema, the locations' symbols in order, and the edit —
    `none`, `passes`, or the gate's reason for withholding it."""

    valid: bool
    symbols: list[str | None]
    edit: str


def judge(answer: str, state: dict, deadline: float = DEADLINE) -> Judged | None:
    """`lotml dev judge` on `answer` about the state's file, laid alone in a scratch directory
    under a fixed name; None when it passed its deadline, failed, or printed no verdict (R4.3,
    R4.4)."""
    args = ["dev", "judge", "--answer", "answer.json", "--path", str(state["path"])]
    failing = state.get("failing")
    if failing:
        args += ["--failing", str(failing["name"])]
    scratch = tempfile.TemporaryDirectory(prefix="lotml-reward-", ignore_cleanup_errors=True)
    with scratch as directory:
        root = Path(directory)
        safe.lay(root, {FILE: state["text"]})
        (root / "answer.json").write_text(answer, encoding="utf-8", newline="")
        done = safe.lotml(args, [FILE], root, deadline)
    if done is None or done.returncode != 0:
        return None
    try:
        verdict = json.loads(done.stdout.strip().splitlines()[-1])
        return Judged(bool(verdict["valid"]), list(verdict["symbols"]), str(verdict["edit"]))
    except (json.JSONDecodeError, IndexError, KeyError, TypeError):
        return None


def score(judged: Judged | None, truth: list[str | None]) -> float:
    """Zero for an answer outside the schema or not judged (R4.1, R4.3); else the mean of its
    locations — one when the first names a declaration the fix changed, one half when only a later
    one does — and its edit, one when it passes (R4.2)."""
    if judged is None or not judged.valid:
        return 0.0
    first, later = judged.symbols[:1], judged.symbols[1:]
    if first and first[0] in truth:
        located = 1.0
    elif any(symbol in truth for symbol in later):
        located = 0.5
    else:
        located = 0.0
    return (located + (1.0 if judged.edit == PASSES else 0.0)) / 2


class Reward:
    """A reward function as TRL's `GRPOTrainer` calls it: the group's completions, with the
    records' `state` and `truth` columns — values, or their JSON text as a dataset holds them —
    scored in parallel; `unjudged` counts the answers the judge could not score."""

    __name__ = "compiler"

    def __init__(self, workers: int = 8, deadline: float = DEADLINE) -> None:
        self.workers = workers
        self.deadline = deadline
        self.unjudged = 0
        self._lock = threading.Lock()

    def __call__(
        self, completions: list, state: list[dict], truth: list[list], **_: object
    ) -> list[float]:
        texts = [c[-1]["content"] if isinstance(c, list) else str(c) for c in completions]
        state = [json.loads(s) if isinstance(s, str) else s for s in state]
        truth = [json.loads(t) if isinstance(t, str) else t for t in truth]
        with ThreadPoolExecutor(max_workers=self.workers) as pool:
            verdicts = list(
                pool.map(
                    lambda job: judge(job[0], job[1], self.deadline),
                    zip(texts, state, strict=True),
                )
            )
        with self._lock:
            self.unjudged += sum(1 for v in verdicts if v is None)
        return [score(v, t) for v, t in zip(verdicts, truth, strict=True)]
