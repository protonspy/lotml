"""The Python→lotml corpus pipeline: rules for the mechanical part, a frontier model with the
compiler for the rest, tests to validate.

Each task's canonical typed Python is first translated by the rules (`corpus.rules`). A rule
translation that checks and passes the task's hidden tests is kept as it is. Otherwise a model
is given the Python, the reference, and what the rules wrote and why it failed, and answers up to
`phase1.ROUNDS` times, each after the compiler's diagnostics or the failing tests. A program
enters the corpus only when it passes the hidden tests; it is stored with those tests written
as a lotml `test` block, which `lotml test` must pass too.

    python -m lotml_harness.corpus.pipeline --model claude:sonnet
    python -m lotml_harness.corpus.pipeline --rules-only
"""

import argparse
import json
import statistics
import sys
import tempfile
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from lotml_harness.corpus import rules
from lotml_harness.experiments import phase1, variants
from lotml_harness.experiments.models import Model, from_spec
from lotml_harness.experiments.phase1 import RESULTS, Lotml, Verdict
from lotml_harness.tasks import Task, build, types, values

RUNS = RESULTS / "corpus"
CORPUS = RUNS / "corpus.jsonl"
REPORT = RESULTS / "corpus.md"
RULES = "rules"
"""The run that asks no model: the rules alone."""

INSTRUCTIONS = (
    "Translate this Python into lotml. Keep the function's name, and give it exactly this "
    "signature: `{signature}`. Answer with the whole lotml program in one ```lotml code block, "
    "and no explanation."
)


class Translate:
    """The translation arm: the Python and what the rules made of it in, `Lotml.judge` out."""

    def __init__(self, drafts: dict[str, tuple[rules.Translation, Verdict | None]], judge: Lotml):
        self.drafts = drafts
        self.lotml = judge

    def prompt(self, task: Task) -> tuple[str, str]:
        system = (
            "You translate Python into lotml, a programming language described by this "
            "reference. Write only lotml.\n\n" + variants.reference_text("b")
        )
        signature = task.signature("b").removesuffix(":")
        user = (
            INSTRUCTIONS.format(signature=signature)
            + f"\n\n```python\n{task.canonical.strip()}\n```\n"
        )
        draft, verdict = self.drafts[task.id]
        if draft.code is None:
            user += "\nA mechanical translation stopped at: " + "; ".join(draft.reasons) + ".\n"
        elif verdict is not None:
            user += (
                "\nA mechanical translation, which you may start from:\n\n"
                f"```lotml\n{draft.code}```\n\n{verdict.feedback}\n"
            )
        return system, user

    def judge(self, task: Task, code: str) -> Verdict:
        return self.lotml.judge(task, code)


def translate(task: Task, model: Model | None, judge: Lotml) -> dict:
    """One task through the pipeline: the rules, then the model if they did not pass."""
    draft = rules.translate(task)
    record = {
        "task": task.id,
        "source": task.source,
        "model": model.name if model else RULES,
        "reasons": draft.reasons,
    }
    verdict = judge.judge(task, draft.code) if draft.code is not None else None
    if verdict is not None and verdict.passed:
        return record | {"error": None, "origin": "rules", "code": draft.code, "rounds": []}
    if model is None:
        outcome = verdict.outcome if verdict else "unsupported"
        return record | {"error": None, "origin": None, "outcome": outcome, "rounds": []}
    arm = Translate({task.id: (draft, verdict)}, judge)
    conversation = phase1.converse(model, arm, task, "lotml")
    if conversation.get("error"):
        return record | {"error": conversation["error"], "rounds": conversation["rounds"]}
    green = conversation.get("green")
    code = conversation["rounds"][green - 1]["code"] if green else None
    return record | {
        "error": None,
        "origin": "model" if green else None,
        "code": code,
        "green": green,
        "rounds": conversation["rounds"],
    }


def assertion(task: Task, case) -> str | None:
    """A hidden test as a lotml assertion, or None for one compared as a set or as lines."""
    args = ", ".join(
        values.render(values.conform(a, t), t, "b")
        for a, (_, t) in zip(case.args, task.params, strict=True)
    )
    call = f"{task.name}({args})"
    expected = values.render(values.conform(case.expected, task.returns), task.returns, "b")
    if case.compare == "eq":
        return f"assert {call} == {expected}"
    if case.compare == "approx" and task.returns == types.Prim("f64"):
        return f"assert abs({call} - {expected}) < 0.000001"
    return None


def with_tests(task: Task, code: str) -> tuple[str, int]:
    """The program with its hidden tests written after it as a `test` block, and how many."""
    lines = []
    for case in task.tests:
        try:
            line = assertion(task, case)
        except values.Mismatch:
            line = None
        if line is not None:
            lines.append("    " + line)
    if not lines:
        return code, 0
    return code.rstrip("\n") + f'\n\ntest "{task.name}":\n' + "\n".join(lines) + "\n", len(lines)


def tested(code: str, judge: Lotml) -> bool:
    """Whether `lotml test` passes every test block of a program."""
    with tempfile.TemporaryDirectory(prefix="lotml-corpus-") as directory:
        source = Path(directory) / "program.lotml"
        source.write_text(code, encoding="utf-8")
        result = judge.compiler(["test", "--json", source.name], directory)
    if result is None or result.returncode != 0:
        return False
    try:
        report = json.loads(result.stdout.strip().splitlines()[-1])
    except (json.JSONDecodeError, IndexError):
        return False
    summary = report["summary"]
    return summary["passed"] > 0 and summary["failed"] + summary["errors"] + summary["panics"] == 0


def run(tasks: list[Task], model: Model | None, path: Path, workers: int = 1) -> list[dict]:
    """Every task through the pipeline, resuming from `path`."""
    judge = Lotml()
    done = {}
    if path.exists():
        for line in path.read_text(encoding="utf-8").splitlines():
            record = json.loads(line)
            if record.get("error") is None:
                done[record["task"]] = record
    todo = [t for t in tasks if t.id not in done]
    path.parent.mkdir(parents=True, exist_ok=True)
    with ThreadPoolExecutor(max_workers=workers) as pool:
        for record in pool.map(lambda t: translate(t, model, judge), todo):
            with path.open("a", encoding="utf-8") as out:
                out.write(json.dumps(record, ensure_ascii=False) + "\n")
            if record.get("error") is None:
                done[record["task"]] = record
    return [done[t.id] for t in tasks if t.id in done]


def corpus(tasks: list[Task], records: list[dict], judge: Lotml) -> list[dict]:
    """The accepted programs, each with its tests in a `test` block that `lotml test` passes;
    a program whose written tests do not pass is kept without them, and says so."""
    by_id = {t.id: t for t in tasks}
    entries = []
    for record in records:
        if record.get("origin") is None:
            continue
        task = by_id[record["task"]]
        program, count = with_tests(task, record["code"])
        written = count > 0 and tested(program, judge)
        entries.append(
            {
                "task": task.id,
                "source": task.source,
                "origin": record["origin"],
                "model": record["model"] if record["origin"] == "model" else None,
                "python": task.canonical.strip() + "\n",
                "lotml": program if written else record["code"],
                "tests": count if written else 0,
            }
        )
    return entries


def best(runs: dict[str, list[dict]], tasks: list[Task]) -> list[dict]:
    """For each task, a passing record: the rules' when they passed, else the first model's."""
    chosen = []
    for task in tasks:
        candidates = [r for rs in runs.values() for r in rs if r["task"] == task.id]
        passing = [r for r in candidates if r.get("origin")]
        passing.sort(key=lambda r: (r["origin"] != "rules", r["model"]))
        if passing:
            chosen.append(passing[0])
        elif candidates:
            chosen.append(candidates[0])
    return chosen


def summarize(tasks: list[Task], chosen: list[dict], entries: list[dict]) -> dict:
    by_source: dict[str, Counter] = {}
    for task in tasks:
        by_source.setdefault(task.source, Counter())["tasks"] += 1
    for record in chosen:
        counts = by_source[record["source"]]
        if record.get("origin") == "rules":
            counts["rules"] += 1
        elif record.get("origin") == "model":
            counts["model first" if record.get("green") == 1 else "model after feedback"] += 1
    for entry in entries:
        by_source[entry["source"]]["with tests"] += entry["tests"] > 0
    reasons = Counter(reason for r in chosen for reason in r.get("reasons", []))
    rounds = [r["green"] for r in chosen if r.get("origin") == "model"]
    return {
        "by_source": by_source,
        "reasons": reasons.most_common(10),
        "median_rounds": statistics.median(rounds) if rounds else None,
        "models": sorted({r["model"] for r in chosen if r.get("origin") == "model"}),
    }


def markdown(summary: dict, entries: list[dict]) -> str:
    lines = [
        "# The Python→lotml corpus",
        "",
        "Generated by `python -m lotml_harness.corpus.pipeline`. Each task's canonical typed",
        "Python is translated by the rules where the mapping is mechanical; a translation the",
        "rules could not write, or that did not pass the task's hidden tests, goes to a model with",
        f"the compiler, up to {phase1.ROUNDS} answers after its diagnostics or the failing tests.",
        "A program enters the corpus only when it passes the hidden tests, and is stored with",
        "them as a `test` block that `lotml test` passes.",
        "",
        "| source | tasks | by the rules | by a model, first answer | by a model, after feedback |"
        " not translated | in the corpus | with its tests |",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    total = Counter()
    for source, counts in sorted(summary["by_source"].items()):
        translated = counts["rules"] + counts["model first"] + counts["model after feedback"]
        row = Counter(counts, translated=translated, missing=counts["tasks"] - translated)
        total.update(row)
        lines.append(
            f"| {source} | {counts['tasks']} | {counts['rules']} | {counts['model first']} |"
            f" {counts['model after feedback']} | {row['missing']} | {translated} |"
            f" {counts['with tests']} |"
        )
    lines.append(
        f"| total | {total['tasks']} | {total['rules']} | {total['model first']} |"
        f" {total['model after feedback']} | {total['missing']} | {total['translated']} |"
        f" {total['with tests']} |"
    )
    models = ", ".join(summary["models"]) or "none"
    lines += [
        "",
        f"Models: {models}. Median answers to a passing translation, for those a model wrote:"
        f" {summary['median_rounds']}.",
        "",
        "What kept the rules from writing a translation, most often first:",
        "",
        "| reason | tasks |",
        "| --- | ---: |",
        *(f"| {reason} | {count} |" for reason, count in summary["reasons"]),
        "",
        f"Every one of the {len(entries)} programs in `results/corpus/corpus.jsonl` passed its"
        " task's hidden tests.",
        "",
    ]
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--model", action="append", default=[], help="claude:sonnet")
    parser.add_argument("--rules-only", action="store_true", help="ask no model")
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--limit", type=int, default=None, help="the first N tasks only")
    args = parser.parse_args()
    tasks = [t for t in build.load() if t.canonical][: args.limit]
    runs = {RULES: run(tasks, None, RUNS / f"{RULES}.jsonl", workers=args.workers)}
    if not args.rules_only:
        for spec in args.model:
            model = from_spec(spec)
            runs[model.name] = run(tasks, model, RUNS / f"{model.name}.jsonl", args.workers)
    chosen = best(runs, tasks)
    entries = corpus(tasks, chosen, Lotml())
    CORPUS.write_text(
        "".join(json.dumps(e, ensure_ascii=False) + "\n" for e in entries), encoding="utf-8"
    )
    report = markdown(summarize(tasks, chosen, entries), entries)
    REPORT.write_text(report, encoding="utf-8")
    sys.stdout.buffer.write(report.encode("utf-8"))


if __name__ == "__main__":
    main()
