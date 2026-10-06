"""The trace dataset: what agent runs leave that a compiler-embedded model can learn from
(specs/trace-dataset/).

A repair record is the triple the study behind the guide names — the failing file, what the
compiler or a test said of it, the file that fixed it — with the task's prompt and the lines the
repair changed. The exporter writes real ones from traces and the seeded failures write theirs in
the same shape, `meta.origin` telling them apart.
"""

import difflib
import hashlib
import json
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


def source_of(task: str) -> str:
    """The source a task id says it comes from."""
    if task.startswith("humaneval-"):
        return "humaneval-original"
    if task.startswith("mbpp-"):
        return "mbpp-original"
    return "bench"


def _prompt(trace: dict) -> str:
    for message in trace.get("messages") or []:
        if message.get("type") == "human":
            return str((message.get("data") or {}).get("content", ""))
    return ""


def _meta(trace: dict) -> dict[str, Any]:
    row = trace.get("row") or {}
    return {
        "task": row.get("task"),
        "source": source_of(str(row.get("task", ""))),
        "model": row.get("model"),
        "arm": row.get("arm"),
        "outcome": row.get("outcome"),
        "compiler": row.get("compiler"),
        "origin": "real",
    }


def _counts(call: dict) -> bool:
    """A call opens or closes a repair only when it succeeded, its report parsed, and it saw no
    file change between its start and end."""
    return (
        call.get("status") == "success"
        and isinstance(call.get("report"), dict)
        and not call.get("truncated")
        and call.get("before") == call.get("after")
    )


def _judged(call: dict) -> list[str]:
    """The files a check judged: those named, those under a directory named — `.` names all —
    or every file when it named none."""
    files = sorted(call.get("before") or {})
    named = [str(p).strip("/") for p in call.get("paths") or []]
    if not named or any(n in ("", ".") for n in named):
        return files
    return [f for f in files if any(f == n or f.startswith(n.rstrip("/") + "/") for n in named)]


def _once(found: list[dict]) -> list[dict]:
    """Each identical repair once: by task and the SHA-256 of before, what was said, and after."""
    seen, kept = set(), []
    for record in found:
        said = record["diagnostics"] if record["diagnostics"] is not None else record["failing"]
        text = record["before"] + json.dumps(said, sort_keys=True) + record["after"]
        key = (record["meta"]["task"], hashlib.sha256(text.encode("utf-8")).hexdigest())
        if key not in seen:
            seen.add(key)
            kept.append(record)
    return kept


def check_repairs(trace: dict) -> list[dict]:
    """The repairs a trace's checks show: a judged file with errors, closed by the next counting
    check that judged it and reports none in it — the file at both points, its diagnostics, the
    task's prompt and the changed lines. A file still failing at the end gives nothing; the run's
    outcome does not matter (R3.2-R3.4, R3.6)."""
    pending: dict[str, tuple[str, list]] = {}
    found = []
    for call in trace.get("checks") or []:
        if not _counts(call):
            continue
        diagnostics = call["report"].get("diagnostics") or []
        for name in _judged(call):
            errors = [
                d for d in diagnostics if d.get("file") == name and d.get("severity") == "error"
            ]
            text = call["before"][name]
            if errors:
                pending[name] = (text, errors)
            elif name in pending:
                before, said = pending.pop(name)
                found.append(
                    repair(_prompt(trace), name, before, text, _meta(trace), diagnostics=said)
                )
    return _once(found)


def _file_of(row: dict, files: dict[str, str]) -> str | None:
    """The snapshot's name for the file a test row names, which `test` reports as a full path."""
    shown = str(row.get("file") or "").replace("\\", "/")
    matching = [name for name in files if shown == name or shown.endswith("/" + name)]
    return max(matching, key=len) if matching else None


def block_repairs(trace: dict) -> list[dict]:
    """The repairs a trace's tests show: a block reported failing, closed by the next counting test
    that reports it passing — the file at both points, the block with the values its comparison
    saw, the task's prompt and the changed lines (R3.4, R3.9)."""
    pending: dict[tuple[str, str], tuple[str, dict]] = {}
    found = []
    for call in trace.get("tests") or []:
        if not _counts(call):
            continue
        for row in call["report"].get("tests") or []:
            name = _file_of(row, call["before"])
            if name is None or row.get("name") is None:
                continue
            key = (name, str(row["name"]))
            if row.get("outcome") != "pass":
                pending[key] = (call["before"][name], row)
            elif key in pending:
                before, failing = pending.pop(key)
                after = call["before"][name]
                found.append(
                    repair(_prompt(trace), name, before, after, _meta(trace), failing=failing)
                )
    return _once(found)
