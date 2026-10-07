"""The trace dataset: what agent runs leave that a compiler-embedded model can learn from
(specs/trace-dataset/).

    python -m lotml_harness.agent.dataset

A repair record is the triple the study behind the guide names — the failing file, what the
compiler or a test said of it, the file that fixed it — with the task's prompt and the lines the
repair changed. The exporter writes real ones from traces and the seeded failures write theirs in
the same shape, `meta.origin` telling them apart.
"""

import difflib
import functools
import hashlib
import json
import re
from collections import Counter, defaultdict
from dataclasses import dataclass, field
from pathlib import Path
from typing import TYPE_CHECKING, Any

from lotml_harness.agent.secrets import anonymised, holds_secret, scrub

if TYPE_CHECKING:
    from lotml_harness.agent.bench import AgentTask
    from lotml_harness.agent.registry import Registry


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
    """The source a task id says it comes from, read only from the spellings the agent harness
    writes; any other, though the split maps it to a problem, is `unknown`, which the licence
    registry holds no entry for."""
    from lotml_harness import split

    if re.fullmatch(r"humaneval-\d+", task):
        return "humaneval-original"
    if re.fullmatch(r"mbpp-\d+", task):
        return "mbpp-original"
    return "bench" if task in split.bench_tasks() else "unknown"


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


def _strings(value: Any) -> list[str]:
    match value:
        case str():
            return [value]
        case list():
            return [s for v in value for s in _strings(v)]
        case dict():
            return [s for k, v in value.items() for s in [str(k), *_strings(v)]]
    return []


def _collapsed(text: str) -> str:
    return " ".join(text.split())


def dropped(record: dict, hidden: set[str]) -> str | None:
    """Why a record stays out of the dataset, or None: it holds a secret, or an `assert` line of
    the task's hidden tests — compared with whitespace collapsed — that its prompt does not also
    show (R3.7)."""
    text = "\n".join(_strings(record))
    if holds_secret(text):
        return "a secret"
    asked = record.get("prompt") or next(
        (m.get("content") for m in record.get("messages") or [] if m.get("role") == "user"), ""
    )
    prompt = _collapsed(str(asked))
    collapsed = _collapsed(text)
    for line in hidden:
        wanted = _collapsed(line)
        if _bounded(re.escape(wanted)).search(collapsed) and not _shown(wanted, prompt):
            return "a hidden test"
    return None


SHOWN_BY = r"\s*(?:==|=>|->|➞|→|should return|returns?)?\s*"
"""What a prompt puts between a call and its result: a docstring's line break, collapsed to a
space, or an arrow or a word."""


def _bounded(pattern: str) -> re.Pattern:
    """`pattern` matched whole: no word character on either side, and no decimal part after it,
    so `f(1) == 2` is not found in `f(1) == 20`."""
    return re.compile(rf"(?<![\w.]){pattern}(?!\w|\.\d)")


def _shown(assertion: str, prompt: str) -> bool:
    """Whether the prompt pairs the call an assert makes with the value it expects, as a
    docstring's example does — `>>> f(x)` with the result on the next line, or `f(x) == y` — or
    holds the whole assert. A call and a value apart in the prompt are not an example of it."""
    compared = re.fullmatch(r"assert (.+?) == (.+)", assertion)
    if compared is None:
        return bool(_bounded(re.escape(assertion.removeprefix("assert "))).search(prompt))
    call, expected = compared.groups()
    return bool(_bounded(re.escape(call) + SHOWN_BY + re.escape(expected)).search(prompt))


def _chat(message: dict) -> dict | None:
    """A LangChain message as the OpenAI chat shape most trainers read."""
    data = message.get("data") or {}
    content = data.get("content", "")
    content = content if isinstance(content, str) else json.dumps(content)
    match message.get("type"):
        case "human":
            return {"role": "user", "content": content}
        case "ai":
            said: dict = {"role": "assistant", "content": content}
            calls = [
                {
                    "id": call.get("id"),
                    "type": "function",
                    "function": {
                        "name": call.get("name"),
                        "arguments": json.dumps(call.get("args")),
                    },
                }
                for call in data.get("tool_calls") or []
            ]
            return said | ({"tool_calls": calls} if calls else {})
        case "tool":
            return {"role": "tool", "tool_call_id": data.get("tool_call_id"), "content": content}
    return None


def trajectory(trace: dict) -> dict | None:
    """A passing run as one chat record: its system message, the main agent's messages with their
    tool calls and results, the tools offered, and the files at the end (R3.1, R3.5)."""
    if (trace.get("row") or {}).get("outcome") != "pass":
        return None
    messages = [{"role": "system", "content": trace["system"]}] if trace.get("system") else []
    messages += [m for m in (_chat(m) for m in trace.get("messages") or []) if m is not None]
    return {
        "messages": messages,
        "tools": trace.get("tools") or [],
        "files": trace.get("final") or {},
        "meta": _meta(trace),
    }


BUCKETS = ("train", "validation")
DATASET = Path(__file__).resolve().parents[2] / "cache" / "dataset"
MANIFEST = Path(__file__).resolve().parents[2] / "results" / "dataset.md"


@dataclass
class Manifest:
    """What an export read, wrote and left out, by reason (R1.7, R4.6)."""

    runs: int = 0
    left_out: Counter = field(default_factory=Counter)
    dropped: Counter = field(default_factory=Counter)
    exported: dict = field(default_factory=dict)
    problems: dict = field(default_factory=lambda: defaultdict(set))
    written: Counter = field(default_factory=Counter)


def export(traces: Path, found: "Registry", hidden_for, out: Path) -> Manifest:
    """Every run the registry permits, as trajectories and repairs marked with their problem and
    split, train and validation apart, held-out problems never written (R1.2, R3.8, R4.5)."""
    from lotml_harness import split
    from lotml_harness.agent.registry import identity

    manifest = Manifest()
    kept: dict[tuple[str, str], list[dict]] = defaultdict(list)
    for path in sorted(traces.rglob("*.json")):
        if path.is_symlink():
            continue
        manifest.runs += 1
        try:
            trace = json.loads(path.read_text(encoding="utf-8"))
        except (json.JSONDecodeError, UnicodeDecodeError):
            manifest.left_out["unreadable trace"] += 1
            continue
        if not isinstance(trace, dict):
            manifest.left_out["unreadable trace"] += 1
            continue
        run, why = identity(trace, path, traces)
        if run is None:
            manifest.left_out[why] += 1
            continue
        allowed, why = found.decide(run["source"], run["model"], run["providers"])
        if not allowed:
            manifest.left_out[why] += 1
            continue
        try:
            problem = split.problem(run["task"])
        except ValueError:
            manifest.left_out["unknown problem"] += 1
            continue
        bucket = split.split(problem)
        if bucket not in BUCKETS:
            manifest.left_out["held out"] += 1
            continue
        manifest.exported.setdefault(("source", run["source"]), found.sources[run["source"]])
        manifest.exported.setdefault(("model", run["model"]), found.models[run["model"]])
        for provider in run["providers"]:
            served = found.models[run["model"]]["providers"][provider]
            manifest.exported.setdefault(("provider", f"{run['model']} via {provider}"), served)
        try:
            hidden = hidden_for(run["task"])
        except (LookupError, ValueError):
            manifest.left_out["unknown task"] += 1
            continue
        made = [("trajectories", trajectory(trace))]
        made += [("repairs", r) for r in check_repairs(trace) + block_repairs(trace)]
        for kind, record in made:
            if record is None:
                continue
            why = dropped(record, hidden)
            if why is not None:
                manifest.dropped[why] += 1
                continue
            record = json.loads(anonymised(json.dumps(record)))
            record["meta"] |= {"problem": problem, "split": bucket}
            kept[(bucket, kind)].append(record)
            manifest.written[(run["source"], bucket, kind)] += 1
            manifest.problems[(run["source"], bucket)].add(problem)
    for bucket in BUCKETS:
        (out / bucket).mkdir(parents=True, exist_ok=True)
        for kind in ("trajectories", "repairs"):
            lines = "".join(json.dumps(r, ensure_ascii=False) + "\n" for r in kept[(bucket, kind)])
            (out / bucket / f"{kind}.jsonl").write_text(lines, encoding="utf-8")
    notices = sorted({entry["notice"] for entry in manifest.exported.values() if entry["notice"]})
    (out / "NOTICE").write_text("\n\n".join(notices) + "\n", encoding="utf-8")
    return manifest


def cell(text: object) -> str:
    """`text` as one table cell: a pipe or a line break in a model's or a provider's name cannot
    split the row."""
    return " ".join(str(text).split()).replace("|", "\\|")


def markdown(manifest: Manifest, day: str) -> str:
    """The committed manifest: what was exported with its licence, evidence and notice, what was
    left out and why, and per source and split the problems, trajectories and repairs."""
    lines = [
        "# Trace dataset",
        "",
        f"Written by `python -m lotml_harness.agent.dataset` on {day}: {manifest.runs} runs read.",
        f"The records are in `harness/cache/dataset/{day}/`, git-ignored.",
        "",
        "| exported | name | licence or terms | evidence | notice |",
        "|---|---|---|---|---|",
    ]
    for (kind, name), entry in sorted(manifest.exported.items()):
        terms = entry.get("licence") or entry.get("terms") or ""
        row = (kind, name, terms, entry["evidence"], entry["notice"])
        lines.append("| " + " | ".join(cell(c) for c in row) + " |")
    lines += ["", "| left out | runs |", "|---|---:|"]
    lines += [f"| {cell(why)} | {n} |" for why, n in sorted(manifest.left_out.items())] or [
        "| none | 0 |"
    ]
    lines += ["", "| record dropped | records |", "|---|---:|"]
    lines += [f"| {cell(why)} | {n} |" for why, n in sorted(manifest.dropped.items())] or [
        "| none | 0 |"
    ]
    lines += [
        "",
        "| source | split | problems | trajectories | repairs |",
        "|---|---|---:|---:|---:|",
    ]
    for source, bucket in sorted(manifest.problems):
        lines.append(
            f"| {source} | {bucket} | {len(manifest.problems[(source, bucket)])} | "
            f"{manifest.written[(source, bucket, 'trajectories')]} | "
            f"{manifest.written[(source, bucket, 'repairs')]} |"
        )
    return "\n".join(lines) + "\n"


@functools.cache
def posed(task_id: str) -> "AgentTask":
    """The agent task an id names: the benchmark's, or HumanEval's or MBPP's posed from its
    pinned release, read once per process; LookupError for an id none of them holds."""
    from lotml_harness.agent import humaneval
    from lotml_harness.agent.bench import tasks

    for prefix, read, pose, key in (
        ("humaneval-", humaneval.read_humaneval, humaneval.pose_humaneval, "HumanEval/{}"),
        ("mbpp-", humaneval.read_mbpp, humaneval.pose_mbpp, "{}"),
    ):
        if task_id.startswith(prefix):
            number = task_id.removeprefix(prefix)
            records, _ = read()
            found = [r for r in records if str(r["task_id"]) == key.format(number)]
            if not found:
                raise LookupError(f"no task {task_id}")
            return pose(found[0])
    found = [task for task in tasks() if task.id == task_id]
    if not found:
        raise LookupError(f"no task {task_id}")
    return found[0]


def hidden_asserts(task_id: str) -> set[str]:
    """The assert lines of a task's hidden tests."""
    texts = posed(task_id).hidden_files.values()
    lines = (line.strip() for text in texts for line in text.splitlines())
    return {line for line in lines if line.startswith("assert")}


def main() -> None:
    import datetime

    from lotml_harness.agent.registry import RegistryError, read
    from lotml_harness.agent.run import TRACES

    try:
        found = read()
    except RegistryError as error:
        raise SystemExit(f"the licence registry stops the export: {error}") from error
    day = datetime.date.today().isoformat()
    out = DATASET / day
    manifest = export(TRACES, found, hidden_asserts, out)
    text = anonymised(scrub(markdown(manifest, day)))
    (out / "manifest.md").write_text(text, encoding="utf-8")
    MANIFEST.write_text(text, encoding="utf-8")
    print(text)


if __name__ == "__main__":
    main()
