"""The trace dataset: what agent runs leave that a compiler-embedded model can learn from
(specs/trace-dataset/).

A repair record is the triple the study behind the guide names — the failing file, what the
compiler or a test said of it, the file that fixed it — with the task's prompt and the lines the
repair changed. The exporter writes real ones from traces and the seeded failures write theirs in
the same shape, `meta.origin` telling them apart.
"""

import difflib
from typing import Any


def changed_lines(before: str, after: str) -> list[int]:
    """The lines of `before`, from 1, that the repair changed; for a pure insertion, the line it
    follows, or 1 at the file's start."""
    old, new = before.splitlines(), after.splitlines()
    found: set[int] = set()
    matcher = difflib.SequenceMatcher(a=old, b=new, autojunk=False)
    for tag, i1, i2, _, _ in matcher.get_opcodes():
        if tag == "equal":
            continue
        if i1 == i2:
            found.add(max(i1, 1))
        else:
            found.update(range(i1 + 1, i2 + 1))
    return sorted(found)


def repair(
    prompt: str,
    path: str,
    before: str,
    after: str,
    meta: dict[str, Any],
    diagnostics: list[dict] | None = None,
    failing: dict | None = None,
) -> dict[str, Any]:
    """One repair: the file at the failing point and at the fixed one, with the diagnostics `check`
    gave, or the failing test block with the values each side of its comparison had."""
    return {
        "prompt": prompt,
        "path": path,
        "before": before,
        "after": after,
        "diagnostics": diagnostics,
        "failing": failing,
        "changed": changed_lines(before, after),
        "meta": meta,
    }
