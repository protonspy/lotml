"""Variant B against variant A: the same tasks, the same models, both references.

Each model writes every sampled task once in each variant, from that variant's reference.
An answer is scored by whether it parses, which Python constructs leak into it, which
mutability errors it makes, whether it passes the hidden tests under lotml's semantics, and
whether it would pass if read with Python's — a difference there is an outcome that depends
on the semantics. Pass@1 is compared per model with the exact McNemar test.

    python -m lotml_harness.experiments.variants --model claude:haiku --model ollama:qwen2.5-coder:7b
"""

import argparse
import json
import random
import re
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from math import comb
from pathlib import Path

from lark.exceptions import LarkError

from lotml_harness import ROOT, reference
from lotml_harness.execute import isolated
from lotml_harness.experiments.models import Model, ModelError, from_spec
from lotml_harness.lang.check import leaks, violations
from lotml_harness.lang.grammar import parser
from lotml_harness.tasks import Task, build

RESULTS = ROOT / "harness" / "results"
RUNS = RESULTS / "variants"
REPORT = RESULTS / "variants.md"
SAMPLE = {"humaneval": 100, "mbpp": 100}
"""200 paired tasks: above the 168 that detect 10 points at 20% discordant pairs."""

A_PROSE = [
    ("`import`/`from` and `test`.", "`use` and `test`."),
    (
        "- `T?` is a `T` or `None`; it is the only type that admits `None`. `x is not None` narrows `x`\n"
        "  to `T` inside the block.",
        "- `T?` is a `T` or `none`; it is the only type that admits `none`. `if x:` on a `T?` tests\n"
        '  that it holds a value, even `0` or `""`, and narrows `x` to `T` inside the block;\n'
        "  `x == none` tests for absence.",
    ),
    (
        "- `x ?? default` is the value inside `x`, or `default` when `x` is `None`.",
        "- `x or default` is the value inside `x`, or `default` when `x` is `none`.",
    ),
    (
        "- **Not Python:** no truthiness. `if` and `while` accept only `bool`: write `len(xs) > 0`,\n"
        '  `s != ""`, `x is not None`, `n != 0`. `and`, `or` and `not` take and return `bool`.',
        "- **Not Python:** `if x:` and `x or d` test an optional for absence, not falsiness: `0` and\n"
        '  `""` are values. Otherwise `if` and `while` accept only `bool`: write `len(xs) > 0`,\n'
        '  `s != ""`, `n != 0`.',
    ),
    ("if email is not None:", "if email:"),
    ("comprehensions; `lambda x: expr`.", "comprehensions; `x => expr`."),
    ("**Not Python:** a `lambda` captures copies", "**Not Python:** an `x => …` captures copies"),
    ("`from m import a, b` or `import m`; there is no `import *`.", "`use m.{a, b}` or `use m`."),
    ("compares with `== Ok(v)` or `== Err(e)`.", "compares with `== Ok(v)`, an error `== fail e`."),
    ("`List[…]`, dunder names,", "`List[…]`, `None`, `lambda`, `import`, `from`, `case`, `is`, dunder names,"),
]  # fmt: skip
"""Prose of the reference that variant A says differently, rewritten before the code is."""

A_CODE = [
    (re.compile(r"^(\s*)case ", re.MULTILINE), r"\1"),
    (re.compile(r"\blambda (\w+): "), r"\1 => "),
    (re.compile(r"\?\?"), "or"),
    (re.compile(r"^from (\w+) import (.+)$", re.MULTILINE), r"use \1.{\2}"),
    (re.compile(r"^import (\w+)$", re.MULTILINE), r"use \1"),
    (re.compile(r"== Err\((\w+(?:\([^()]*\))?)\)"), r"== fail \1"),
    (re.compile(r"\bNone\b"), "none"),
]


def reference_text(variant: str) -> str:
    """The reference a model reads: the published one, or its rendering in variant A."""
    text = reference.text()
    if variant == "b":
        return text
    for old, new in A_PROSE:
        if old not in text:
            raise ValueError(f"the reference no longer says: {old!r}")
        text = text.replace(old, new)
    body, marker, excluded = text.partition("\n## Not in the language")
    for pattern, replacement in A_CODE:
        body = pattern.sub(replacement, body)
    return body + marker + excluded


INSTRUCTIONS = (
    "Complete the lotml function below. Answer with the whole function, and any type or helper "
    "function it needs, in one ```lotml code block, with no explanation and no test blocks."
)


def prompt(task: Task, variant: str) -> tuple[str, str]:
    """The system prompt (the reference) and the user prompt (the task) for one variant."""
    system = (
        "You write lotml, a programming language described by this reference. "
        "Write only lotml.\n\n" + reference_text(variant)
    )
    user = f"{INSTRUCTIONS}\n\n```lotml\n{task.prompt(variant)}```\n"
    return system, user


FENCE = re.compile(r"```[^\n`]*\n(.*?)```", re.DOTALL)


def extract(answer: str) -> str:
    """The last fenced code block of an answer, or the whole answer when it has none."""
    blocks = FENCE.findall(answer)
    code = blocks[-1] if blocks else answer
    return code.strip("\n") + "\n"


def score(task: Task, variant: str, code: str) -> dict:
    """What one answer does: parse, leaks, mutability errors, hidden tests in both semantics."""
    lotml = isolated(code, variant, "lotml", task)
    python = isolated(code, variant, "python", task)
    return {
        **static_checks(variant, code),
        "error": None,
        "run_error": lotml.error,
        "cases": lotml.cases,
        "cases_python": python.cases,
        "passed": lotml.passed,
        "passed_python": python.passed,
        "semantics_dependent": lotml.passed != python.passed,
    }


def static_checks(variant: str, code: str) -> dict:
    """The checks made in this process: parse, leaks and mutability errors."""
    try:
        parser(variant).parse(code)
        parse_error = None
    except LarkError as error:
        parse_error = str(error).strip().splitlines()[0]
    return {
        "parse_error": parse_error,
        "leaks": leaks(variant, code),
        "violations": violations(variant, code) if parse_error is None else [],
    }


def refresh(path: Path) -> int:
    """Recompute the in-process checks of every stored answer; how many changed.

    The hidden tests ran in child processes and stand; the parse and mutability checks ran
    in worker threads that, before parsers became per-thread, shared one parser's state.
    """
    lines, changed = path.read_text(encoding="utf-8").splitlines(), 0
    out = []
    for line in lines:
        row = json.loads(line)
        if row.get("error") is None and "code" in row:
            fresh = static_checks(row["variant"], row["code"])
            if any(row.get(k) != v for k, v in fresh.items()):
                changed += 1
            row |= fresh
        out.append(json.dumps(row, ensure_ascii=False))
    path.write_text("".join(o + "\n" for o in out), encoding="utf-8")
    return changed


def mcnemar(b: int, c: int) -> float:
    """Exact two-sided McNemar p-value from the discordant pairs `b` and `c`."""
    n = b + c
    if n == 0:
        return 1.0
    tail = sum(comb(n, k) for k in range(min(b, c) + 1)) / 2**n
    return min(1.0, 2 * tail)


def sample(tasks: list[Task], counts: dict[str, int], seed: int = 0) -> list[Task]:
    """A seeded sample of each source, in a fixed order: the paired tasks of a comparison."""
    chosen = []
    for source, count in counts.items():
        pool = sorted((t for t in tasks if t.source == source), key=lambda t: t.id)
        chosen += random.Random(f"{seed}/{source}").sample(pool, count)  # noqa: S311
    return chosen


def attempt(model: Model, task: Task, variant: str) -> dict:
    system, user = prompt(task, variant)
    record = {"model": model.name, "family": model.family, "variant": variant, "task": task.id}
    try:
        completion = model.complete(system, user)
    except ModelError as error:
        return record | {"error": str(error)}
    code = extract(completion.text)
    return record | {
        "answer": completion.text,
        "code": code,
        "input_tokens": completion.input_tokens,
        "output_tokens": completion.output_tokens,
        "seconds": round(completion.seconds, 2),
        **score(task, variant, code),
    }


def run(
    model: Model,
    tasks: list[Task],
    path: Path,
    workers: int = 1,
    variants: tuple[str, ...] = ("a", "b"),
) -> list[dict]:
    """Every task in every variant, resuming from `path`; failed completions are retried."""
    done: dict[tuple[str, str], dict] = {}
    if path.exists():
        for line in path.read_text(encoding="utf-8").splitlines():
            record = json.loads(line)
            if record.get("error") is None:
                done[(record["task"], record["variant"])] = record
    todo = [(t, v) for t in tasks for v in variants if (t.id, v) not in done]
    path.parent.mkdir(parents=True, exist_ok=True)
    with ThreadPoolExecutor(max_workers=workers) as pool:
        for record in pool.map(lambda job: attempt(model, *job), todo):
            with path.open("a", encoding="utf-8") as out:
                out.write(json.dumps(record, ensure_ascii=False) + "\n")
            done.setdefault((record["task"], record["variant"]), record)
            if record.get("error") is not None:
                done[(record["task"], record["variant"])] = record
    return [done[(t.id, v)] for t in tasks for v in variants if (t.id, v) in done]


def summarize(rows: list[dict]) -> dict[str, dict]:
    """Per model and variant: counts; per model: the paired pass@1 comparison."""
    summary: dict[str, dict] = {}
    for model in dict.fromkeys(r["model"] for r in rows):
        mine = [r for r in rows if r["model"] == model and r.get("error") is None]
        entry: dict = {}
        for variant in ("a", "b"):
            answers = [r for r in mine if r["variant"] == variant]
            entry[variant] = {
                "answers": len(answers),
                "parsed": sum(r["parse_error"] is None for r in answers),
                "leaked": sum(bool(r["leaks"]) for r in answers),
                "leaks": Counter(name for r in answers for name in r["leaks"]),
                "violated": sum(bool(r["violations"]) for r in answers),
                "passed": sum(r["passed"] for r in answers),
                "passed_python": sum(r["passed_python"] for r in answers),
                "semantics_dependent": sum(r["semantics_dependent"] for r in answers),
                "output_tokens": sum(r.get("output_tokens", 0) for r in answers),
            }
        by_task = {(r["task"], r["variant"]): r["passed"] for r in mine}
        tasks = {t for t, v in by_task if (t, "a") in by_task and (t, "b") in by_task}
        entry["pairs"] = len(tasks)
        entry["a_only"] = sum(by_task[(t, "a")] and not by_task[(t, "b")] for t in tasks)
        entry["b_only"] = sum(by_task[(t, "b")] and not by_task[(t, "a")] for t in tasks)
        entry["p"] = mcnemar(entry["a_only"], entry["b_only"])
        summary[model] = entry
    return summary


def percent(part: int, whole: int) -> str:
    return f"{100 * part / whole:.1f}%" if whole else "—"


def markdown(rows: list[dict], families: dict[str, str]) -> str:
    summary = summarize(rows)
    lines = [
        "# Variant B against variant A",
        "",
        "Generated by `python -m lotml_harness.experiments.variants`. Each model wrote every",
        "sampled task once per variant, from that variant's reference (`reference/lotml.md`, and",
        "its rendering in variant A). Hidden tests ran under lotml's semantics and, for the",
        "last columns, as Python would read the same program.",
        "",
        "## Pass@1, paired",
        "",
        "| model | family | pairs | A passes | B passes | only A | only B | McNemar p |",
        "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    for model, entry in summary.items():
        a, b = entry["a"], entry["b"]
        lines.append(
            f"| {model} | {families.get(model, '')} | {entry['pairs']} "
            f"| {percent(a['passed'], a['answers'])} | {percent(b['passed'], b['answers'])} "
            f"| {entry['a_only']} | {entry['b_only']} | {entry['p']:.3f} |"
        )
    lines += [
        "",
        "## Syntax and semantics, per variant",
        "",
        "| model | variant | answers | parse | leaked | mutability errors | passes as Python "
        "| outcome depends on semantics | output tokens |",
        "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    for model, entry in summary.items():
        for variant in ("a", "b"):
            v = entry[variant]
            lines.append(
                f"| {model} | {variant.upper()} | {v['answers']} "
                f"| {percent(v['parsed'], v['answers'])} | {v['leaked']} | {v['violated']} "
                f"| {percent(v['passed_python'], v['answers'])} | {v['semantics_dependent']} "
                f"| {v['output_tokens']} |"
            )
    lines += ["", "## Leaked constructs", "", "| model | variant | construct | answers |"]
    lines += ["| --- | --- | --- | ---: |"]
    for model, entry in summary.items():
        for variant in ("a", "b"):
            for name, count in entry[variant]["leaks"].most_common():
                lines.append(f"| {model} | {variant.upper()} | `{name}` | {count} |")
    errors = Counter(r["model"] for r in rows if r.get("error") is not None)
    if errors:
        lines += ["", "Completions that never came back: "]
        lines[-1] += ", ".join(f"{m} {n}" for m, n in errors.items()) + "."
    return "\n".join(lines) + "\n"


def collected(directory: Path, tasks: list[Task]) -> list[dict]:
    """Every model's answers stored under `directory`, for the sampled tasks."""
    wanted = {t.id for t in tasks}
    rows: dict[tuple[str, str, str], dict] = {}
    for path in sorted(directory.glob("*.jsonl")):
        for line in path.read_text(encoding="utf-8").splitlines():
            record = json.loads(line)
            key = (record["model"], record["task"], record["variant"])
            if record["task"] in wanted and (key not in rows or record.get("error") is None):
                rows[key] = record
    return list(rows.values())


def main() -> None:
    options = argparse.ArgumentParser(description=__doc__)
    options.add_argument("--model", action="append", default=[], help="claude:haiku, ...")
    options.add_argument("--workers", type=int, default=1)
    options.add_argument("--seed", type=int, default=0)
    options.add_argument("--refresh", action="append", default=[], help="a stored model's name")
    arguments = options.parse_args()
    for name in arguments.refresh:
        print(f"{name}: {refresh(RUNS / f'{name}.jsonl')} answers changed")
    tasks = sample(build.load(), SAMPLE, arguments.seed)
    for spec in arguments.model:
        model = from_spec(spec)
        run(model, tasks, RUNS / f"{model.name}.jsonl", arguments.workers)
    rows = collected(RUNS, tasks)
    families = {r["model"]: r["family"] for r in rows}
    REPORT.write_text(markdown(rows, families), encoding="utf-8")
    print(REPORT.read_text(encoding="utf-8"))


if __name__ == "__main__":
    main()
