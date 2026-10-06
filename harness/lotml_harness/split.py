"""The problem split: train, validation or held-out, fixed per original problem.

Every id derived from a problem — the task set's, the agent harness's, MultiPL-E's — maps to the
original benchmark's id first, so a run, a translation and a seeded mutant of one problem always
land together (specs/trace-dataset/ R4.1-R4.4). The exporter, the seeded failures and the guide's
evaluation all ask here.
"""

import re
from hashlib import sha256

from lotml_harness.agent.bench import BENCH

SALT = "lotml-split-1"
"""Fixed for good: a new salt is a new split, and a guide trained on the old one would leak."""
TRAIN, VALIDATION = 0.60, 0.75
"""HumanEval's buckets: below 0.60 train, below 0.75 validation, the rest held out."""

FORMS = [
    (re.compile(r"humaneval/(\d+)"), "humaneval/{}"),
    (re.compile(r"humaneval-(\d+)"), "humaneval/{}"),
    (re.compile(r"HumanEval_(\d+)_\w+"), "humaneval/{}"),
    (re.compile(r"mbpp/(\d+)"), "mbpp/{}"),
    (re.compile(r"mbpp-(\d+)"), "mbpp/{}"),
    (re.compile(r"mbpp_(\d+)_\w+"), "mbpp/{}"),
]


def bench_tasks() -> set[str]:
    return {d.name for d in BENCH.iterdir() if (d / "task.toml").is_file()}


def problem(ident: str) -> str:
    """The original problem `ident` derives from: `humaneval/<n>`, `mbpp/<n>` or
    `bench/<task>`. Any other id is refused, so a new source fails here rather than in train."""
    for pattern, form in FORMS:
        match = pattern.fullmatch(ident)
        if match:
            return form.format(int(match.group(1)))
    if ident in bench_tasks():
        return f"bench/{ident}"
    raise ValueError(f"{ident!r} names no known problem")


def split(problem_id: str) -> str:
    """`train`, `validation` or `held-out` for an original problem's id."""
    if re.fullmatch(r"humaneval/\d+", problem_id):
        share = int.from_bytes(sha256((SALT + problem_id).encode()).digest()[:8]) / 2**64
        return "train" if share < TRAIN else "validation" if share < VALIDATION else "held-out"
    if re.fullmatch(r"mbpp/\d+", problem_id):
        return "held-out"
    if problem_id.startswith("bench/") and problem_id.removeprefix("bench/") in bench_tasks():
        return "train"
    raise ValueError(f"{problem_id!r} names no known problem")
