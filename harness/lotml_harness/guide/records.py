"""Guidance records: every repair — real ones from the trace dataset's export and the seeded
failures — as a chat example the guide is trained on (specs/guide-records/).

    uv run --group guide python -m lotml_harness.guide.records \\
        --tokenizer Qwen/Qwen2.5-Coder-0.5B-Instruct --context 8192 --answer 512

The state is rendered by `lotml guide render`, the code the guide tool runs at inference, so the
prompt trained on is the prompt served; the target is where `lotml dev diff` says the fix falls,
the kind of change and the edit that makes it.
"""

import argparse
import datetime
import json
import tempfile
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from pathlib import Path

from lotml_harness import split
from lotml_harness.agent import safe
from lotml_harness.experiments.phase1 import RESULTS
from lotml_harness.tasks.sources import CACHE

RECORDS = CACHE / "guide" / "records"
SEEDED = CACHE / "guide" / "seeded"
DATASET = CACHE / "dataset"
REPORT = RESULTS / "guide-records.md"
LOCATIONS = 3
"""Locations a target names at most, as the answer's schema allows."""
OVERHEAD = 16
"""Tokens a chat template adds around each message, counted on top of their text."""
BUCKETS = ("train", "validation")


class HeldOut(ValueError):
    """A repair of a held-out problem: the guide's evaluation would see its own training data."""


def latest(root: Path) -> Path | None:
    found = sorted(p for p in root.glob("*") if p.is_dir()) if root.is_dir() else []
    return found[-1] if found else None


def repairs(export: Path | None) -> list[dict]:
    """The repairs an export holds, train and validation."""
    found: list[dict] = []
    for bucket in BUCKETS:
        path = (export / bucket / "repairs.jsonl") if export else None
        if path is not None and path.is_file():
            lines = path.read_text(encoding="utf-8").splitlines()
            found += [json.loads(line) for line in lines if line.strip()]
    return found


def target(diff: dict, path: str) -> dict:
    """What the guide should answer: the changed declarations as locations in the failing file, at
    most three, the kind of change the edit is — `several` when no single edit makes it — and the
    edit as its tool's call, or null."""
    locations = [
        {"path": path, "symbol": d["symbol"], "lines": d["lines"]}
        for d in diff["declarations"][:LOCATIONS]
    ]
    edit = diff["edit"]
    shown = {"tool": edit["tool"], "arguments": edit["arguments"]} if edit else None
    return {"locations": locations, "kind": edit["kind"] if edit else "several", "edit": shown}


def state(repair: dict, task: bool) -> dict:
    """The guide tool's state for a repair: the failing file, and its diagnostics or the failing
    block with its values; the task's prompt when `task`."""
    return {
        "task": repair.get("prompt") if task else None,
        "path": repair["path"],
        "text": repair["before"],
        "diagnostics": repair.get("diagnostics") or [],
        "failing": repair.get("failing"),
    }


@dataclass
class Budget:
    """The base model's context and the answer's `max_tokens`, counted with its own tokenizer —
    the rule the guide server applies at inference."""

    count: object
    context: int
    answer: int

    def fits(self, messages: list[dict]) -> bool:
        used = sum(len(self.count(m["content"])) + OVERHEAD for m in messages)
        return used + self.answer <= self.context


def build(
    repair: dict, budget: Budget, deadline: float = safe.DEADLINE
) -> tuple[list[dict], list[str]]:
    """A repair's guidance records — with the task and without it — and why any was left out.
    Each keeps the state it was rendered from, for judging an answer against the raw file."""
    meta = repair["meta"]
    problem = meta.get("problem") or split.problem(meta["task"])
    bucket = split.split(problem)
    if bucket not in BUCKETS:
        raise HeldOut(f"{meta.get('task')} is {problem}, which the split holds out")
    scratch = tempfile.TemporaryDirectory(prefix="lotml-records-", ignore_cleanup_errors=True)
    with scratch as directory:
        root = Path(directory)
        try:
            safe.lay(root, {"before.lotml": repair["before"], "after.lotml": repair["after"]})
        except ValueError:
            return [], ["unsafe files"]
        args = ["dev", "diff", "--path", repair["path"], "--json"]
        diffed = safe.lotml(args, ["before.lotml", "after.lotml"], root, deadline)
        if diffed is None or diffed.returncode != 0:
            return [], ["diff failed"]
        diff = json.loads(diffed.stdout)
        if not diff["declarations"]:
            return [], ["no change"]
        answer = json.dumps(target(diff, repair["path"]), ensure_ascii=False)
        records, left_out = [], []
        for task in (True, False) if repair.get("prompt") else (False,):
            (root / "state.json").write_text(json.dumps(state(repair, task)), encoding="utf-8")
            rendered = safe.lotml(["guide", "render"], ["state.json"], root, deadline)
            if rendered is None or rendered.returncode != 0:
                return [], ["render failed"]
            shown = json.loads(rendered.stdout)
            messages = shown["messages"]
            if not budget.fits(messages):
                left_out.append("over the context")
                continue
            records.append(
                {
                    "messages": [*messages, {"role": "assistant", "content": answer}],
                    "state": state(repair, task),
                    "meta": {
                        "problem": problem,
                        "split": bucket,
                        "origin": meta.get("origin", "real"),
                        "source": meta.get("source"),
                        "model": meta.get("model"),
                        "compiler": meta.get("compiler"),
                        "renderer": shown["renderer"],
                        "task": task,
                        "kind": json.loads(answer)["kind"],
                    },
                }
            )
    return records, left_out


@dataclass
class Tally:
    records: Counter = field(default_factory=Counter)
    left_out: Counter = field(default_factory=Counter)


def build_all(found: list[dict], budget: Budget, workers: int = 8) -> tuple[list[dict], Tally]:
    """Every repair's records, and the tally of what was written and left out by reason."""
    with ThreadPoolExecutor(max_workers=workers) as pool:
        built = list(pool.map(lambda r: build(r, budget), found))
    tally, records = Tally(), []
    for made, why in built:
        tally.left_out.update(why)
        for record in made:
            m = record["meta"]
            tally.records[(m["origin"], m["split"], m["kind"], m["source"], m["task"])] += 1
            records.append(record)
    return records, tally


def write(records: list[dict], day: str, root: Path = RECORDS) -> Path:
    """Train and validation to separate files in the git-ignored cache."""
    out = root / day
    out.mkdir(parents=True, exist_ok=True)
    for bucket in BUCKETS:
        kept = [r for r in records if r["meta"]["split"] == bucket]
        lines = "".join(json.dumps(r, ensure_ascii=False) + "\n" for r in kept)
        (out / f"{bucket}.jsonl").write_text(lines, encoding="utf-8")
    return out


def markdown(tally: Tally, budget: Budget, tokenizer: str, sources: list[str], day: str) -> str:
    """The committed report: records by origin, split, kind and source, and every one left out
    by reason (R2.7)."""
    lines = [
        "# Guidance records",
        "",
        f"Written by `python -m lotml_harness.guide.records` on {day} from {', '.join(sources)};",
        f"counted with `{tokenizer}`'s tokenizer against a context of {budget.context} tokens with",
        f"{budget.answer} for the answer. The records are in `harness/cache/guide/records/{day}/`,",
        "git-ignored.",
        "",
    ]
    for title, index in (("origin", 0), ("split", 1), ("kind", 2), ("source", 3)):
        counts: Counter = Counter()
        for key, n in tally.records.items():
            counts[str(key[index])] += n
        rows = [f"| {k} | {n} |" for k, n in sorted(counts.items())]
        lines += [f"| {title} | records |", "|---|---:|", *rows, ""]
    with_task = sum(n for key, n in tally.records.items() if key[4])
    left_out = [f"| {why} | {n} |" for why, n in sorted(tally.left_out.items())] or ["| none | 0 |"]
    lines += [
        f"{sum(tally.records.values())} records, {with_task} of them with the task.",
        "",
        "| left out | records |",
        "|---|---:|",
        *left_out,
        "",
    ]
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--tokenizer", required=True, help="the base model's, on Hugging Face")
    parser.add_argument("--context", type=int, required=True, help="the base model's context")
    parser.add_argument("--answer", type=int, required=True, help="the answer's max_tokens")
    parser.add_argument("--workers", type=int, default=8)
    args = parser.parse_args(argv)
    from tokenizers import Tokenizer

    tokenizer = Tokenizer.from_pretrained(args.tokenizer)
    budget = Budget(lambda text: tokenizer.encode(text).ids, args.context, args.answer)
    exports = [p for p in (latest(DATASET), latest(SEEDED)) if p is not None]
    found = [r for export in exports for r in repairs(export)]
    records, tally = build_all(found, budget, args.workers)
    day = datetime.date.today().isoformat()
    out = write(records, day)
    sources = [str(p.relative_to(CACHE.parent.parent)).replace("\\", "/") for p in exports]
    text = markdown(tally, budget, args.tokenizer, sources or ["no export"], day)
    REPORT.write_text(text, encoding="utf-8")
    print(text)
    print(f"{len(records)} records in {out}")


if __name__ == "__main__":
    main()
