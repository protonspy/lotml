"""The forgotten-`await` hypothesis (docs/wiki/pages/colorless-concurrency.md): no source measures
how often a model forgets an `await` or tangles sync and async code. Here each model writes the
same concurrency tasks twice — with Python's asyncio, given helper coroutines, and in lotml,
given helper functions and the colorless `parallel` — and each answer is judged by its tests
and for the concurrency mistakes its language allows.

In Python: a coroutine never awaited, or used as if it were its value (a forgotten `await`);
`await` outside an `async def`, or an event loop started inside a running one (colour). In
lotml: `async` or `await` written anyway, or calls given to `parallel` where tasks are wanted.
An answer can also be right and not concurrent: awaited one by one, or called in a loop.

    python -m lotml_harness.experiments.awaits --model claude:haiku --model ollama:llama3.1:8b
"""

import argparse
import ast
import json
import re
import subprocess
import sys
import tempfile
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from pathlib import Path

from lotml_harness.execute import child_environment
from lotml_harness.experiments import variants
from lotml_harness.experiments.models import Model, ModelError, from_spec
from lotml_harness.experiments.phase1 import RESULTS, Lotml
from lotml_harness.tasks import Case, Task, types

RUNS = RESULTS / "awaits"
REPORT = RESULTS / "awaits.md"
LANGUAGES = ("python", "lotml")
TIMEOUT = 30
"""Seconds an answer's tests may run before it is given up on."""

# A domain: a helper's name, its parameter, and the value it returns, the same formula written in
# Python and in lotml, so the two languages compute the same thing.
DOMAINS = [
    ("price", "item", "(len(item) * 7 + ord(item[0])) % 50 + 1"),
    ("stock", "item", "sum([ord(c) for c in item]) % 23"),
    ("score", "name", "(len(name) * 13) % 31 + ord(name[-1]) % 5"),
    ("weight", "part", "(ord(part[0]) * 3 + len(part)) % 40"),
    ("delay", "job", "sum([ord(c) * (i + 1) for i, c in enumerate(job)]) % 17"),
]

KEYS = [["apple", "pear", "fig"], ["kiwi"], ["a", "bb", "ccc", "dddd"], ["melon", "grape", "lime"]]
"""The keys each task's hidden tests pass; none is empty, so `largest` always has an answer."""

LIST_STR = types.List(types.Prim("str"))
INT = types.Prim("int")


@dataclass
class ConcurrencyTask:
    id: str
    template: str
    name: str
    helper_names: list[str]
    python_helpers: str
    lotml_helpers: str
    python_signature: str
    lotml_signature: str
    python_canonical: str
    lotml_canonical: str
    task: Task


@dataclass
class Judged:
    passed: bool
    concurrent: bool
    mistake: str | None
    detail: str = ""
    cases: list[str] = field(default_factory=list)


def python_helper(name: str, param: str, formula: str) -> str:
    return (
        f"async def {name}({param}: str) -> int:\n"
        f"    await asyncio.sleep(0)\n"
        f"    return {formula}\n"
    )


def lotml_helper(name: str, param: str, formula: str) -> str:
    return f"fn {name}({param}: str) -> int:\n    return {formula}\n"


def value(formula: str, param: str, key: str) -> int:
    return eval(formula, {"ord": ord, "len": len, "sum": sum, "enumerate": enumerate}, {param: key})  # noqa: S307


# Each template: the goal's name, extra parameters, return type, documentation, the reference
# result from the helpers' values, and the canonical solutions in both languages. `{a}` and
# `{b}` are the helpers.
TEMPLATES = {
    "fetch_all": (
        [],
        types.List(INT),
        "The value of each key, in the order of the keys.",
        lambda vals, keys, extra: vals[0],
        "    return list(await asyncio.gather(*({a}(k) for k in keys)))\n",
        "    return parallel([lambda: {a}(k) for k in keys])\n",
    ),
    "total": (
        [],
        INT,
        "The sum of every key's value.",
        lambda vals, keys, extra: sum(vals[0]),
        "    return sum(await asyncio.gather(*({a}(k) for k in keys)))\n",
        "    return sum(parallel([lambda: {a}(k) for k in keys]))\n",
    ),
    "largest": (
        [],
        types.Prim("str"),
        "The key with the largest value; the first such key on a tie.",
        lambda vals, keys, extra: keys[vals[0].index(max(vals[0]))],
        "    values = await asyncio.gather(*({a}(k) for k in keys))\n"
        "    return keys[values.index(max(values))]\n",
        "    values = parallel([lambda: {a}(k) for k in keys])\n"
        "    var best = 0\n"
        "    for i in range(len(values)):\n"
        "        if values[i] > values[best]:\n"
        "            best = i\n"
        "    return keys[best]\n",
    ),
    "count_even": (
        [],
        INT,
        "How many keys have an even value.",
        lambda vals, keys, extra: sum(1 for v in vals[0] if v % 2 == 0),
        "    values = await asyncio.gather(*({a}(k) for k in keys))\n"
        "    return sum(1 for v in values if v % 2 == 0)\n",
        "    values = parallel([lambda: {a}(k) for k in keys])\n"
        "    return len([v for v in values if v % 2 == 0])\n",
    ),
    "combined": (
        [],
        types.List(INT),
        "For each key, in order, the sum of its two values.",
        lambda vals, keys, extra: [x + y for x, y in zip(vals[0], vals[1], strict=True)],
        "    xs = await asyncio.gather(*({a}(k) for k in keys))\n"
        "    ys = await asyncio.gather(*({b}(k) for k in keys))\n"
        "    return [x + y for x, y in zip(xs, ys)]\n",
        "    return parallel([lambda: {a}(k) + {b}(k) for k in keys])\n",
    ),
    "above": (
        [("limit", INT)],
        LIST_STR,
        "The keys whose value is above `limit`, in order.",
        lambda vals, keys, extra: [k for k, v in zip(keys, vals[0], strict=True) if v > extra[0]],
        "    values = await asyncio.gather(*({a}(k) for k in keys))\n"
        "    return [k for k, v in zip(keys, values) if v > limit]\n",
        "    values = parallel([lambda: {a}(k) for k in keys])\n"
        "    return [keys[i] for i in range(len(keys)) if values[i] > limit]\n",
    ),
}


def tasks() -> list[ConcurrencyTask]:
    """Every template over every domain: 30 tasks, each in both languages."""
    found = []
    for template, (extra, returns, doc, reference, python_body, lotml_body) in TEMPLATES.items():
        for i, (a, param_a, formula_a) in enumerate(DOMAINS):
            b, param_b, formula_b = DOMAINS[(i + 1) % len(DOMAINS)]
            helpers = [(a, param_a, formula_a)] + (
                [(b, param_b, formula_b)] if template == "combined" else []
            )
            name = f"{template}_{a}"
            params = [("keys", LIST_STR), *extra]
            cases = []
            for n, keys in enumerate(KEYS):
                extra_args = [10 + n] if extra else []
                vals = [[value(f, p, k) for k in keys] for _, p, f in helpers]
                cases.append(Case([keys, *extra_args], reference(vals, keys, extra_args)))
            python_params = ", ".join(["keys: list[str]", *(f"{n}: int" for n, _ in extra)])
            lotml_params = ", ".join(f"{n}: {t.render('b')}" for n, t in params)
            py_returns = {"[int]": "list[int]", "[str]": "list[str]"}.get(
                returns.render("b"), returns.render("b")
            )
            python_signature = (
                f'async def {name}({python_params}) -> {py_returns}:\n    """{doc}"""\n'
            )
            lotml_signature = (
                f'fn {name}({lotml_params}) -> {returns.render("b")}:\n    """{doc}"""\n'
            )
            fill = {"a": a, "b": b}
            found.append(
                ConcurrencyTask(
                    id=f"awaits/{name}",
                    template=template,
                    name=name,
                    helper_names=[h for h, _, _ in helpers],
                    python_helpers="import asyncio\n\n\n"
                    + "\n\n".join(python_helper(*h) for h in helpers),
                    lotml_helpers="\n".join(lotml_helper(*h) for h in helpers),
                    python_signature=python_signature,
                    lotml_signature=lotml_signature,
                    python_canonical="import asyncio\n\n\n"
                    + python_signature
                    + python_body.format(**fill),
                    lotml_canonical=lotml_signature + lotml_body.format(**fill),
                    task=Task(
                        id=f"awaits/{name}",
                        source="awaits",
                        name=name,
                        params=params,
                        returns=returns,
                        doc=doc,
                        tests=cases,
                    ),
                )
            )
    return found


# Python ---------------------------------------------------------------------------------------

CHILD = r"""
import asyncio, inspect, json, sys, warnings
from lotml_harness.execute import MEMORY, limit_memory
limit_memory(MEMORY)
job = json.loads(sys.stdin.read())
namespace = {}
results = []
try:
    exec(compile(job["helpers"], "<helpers>", "exec"), namespace)
    exec(compile(job["code"], "<answer>", "exec"), namespace)
except BaseException as error:
    print(json.dumps({"load": type(error).__name__ + ": " + str(error)}))
    sys.exit(0)
function = namespace.get(job["name"])
for args, expected in job["cases"]:
    with warnings.catch_warnings(record=True) as seen:
        warnings.simplefilter("always")
        try:
            result = function(*args)
            if inspect.iscoroutine(result):
                result = asyncio.run(result)
            import gc; gc.collect()
            outcome = "pass" if result == expected else "wrong: " + repr(result)[:200]
        except BaseException as error:
            outcome = type(error).__name__ + ": " + str(error)[:200]
    results.append({"outcome": outcome, "warnings": [str(w.message)[:200] for w in seen]})
print(json.dumps({"cases": results}))
"""

CONCURRENT_PY = {"gather", "create_task", "TaskGroup", "as_completed", "wait", "ensure_future"}


def concurrent_python(code: str) -> bool:
    try:
        tree = ast.parse(code)
    except SyntaxError:
        return False
    for node in ast.walk(tree):
        if isinstance(node, ast.Call):
            func = node.func
            name = func.attr if isinstance(func, ast.Attribute) else getattr(func, "id", None)
            if name in CONCURRENT_PY:
                return True
    return False


def judge_python(task: ConcurrencyTask, code: str) -> Judged:
    """The answer run on the hidden tests in a child, and the concurrency mistake it made."""
    job = {
        "helpers": task.python_helpers,
        "code": code,
        "name": task.name,
        "cases": [[c.args, c.expected] for c in task.task.tests],
    }
    try:
        with tempfile.TemporaryDirectory(prefix="lotml-awaits-") as scratch:
            child = subprocess.run(  # noqa: S603
                [sys.executable, "-c", CHILD],
                input=json.dumps(job),
                capture_output=True,
                text=True,
                encoding="utf-8",
                env=child_environment(),
                cwd=scratch,
                timeout=TIMEOUT,
                check=False,
            )
        report = json.loads(child.stdout.strip().splitlines()[-1])
    except (subprocess.TimeoutExpired, json.JSONDecodeError, IndexError):
        return Judged(False, concurrent_python(code), None, "did not finish")
    concurrent = concurrent_python(code)
    if "load" in report:
        load = report["load"]
        colour = "outside async function" in load or "'await' outside" in load
        return Judged(False, concurrent, "colour" if colour else None, load)
    outcomes = [c["outcome"] for c in report["cases"]]
    texts = " ".join(outcomes + [w for c in report["cases"] for w in c["warnings"]])
    mistake = None
    if "cannot be called from a running event loop" in texts or "outside async function" in texts:
        mistake = "colour"
    elif "never awaited" in texts or "coroutine" in texts or "Future" in texts or "<Task" in texts:
        mistake = "forgotten await"
    passed = all(o == "pass" for o in outcomes)
    return Judged(passed, concurrent, mistake, texts[:500], outcomes)


# lotml ----------------------------------------------------------------------------------------


def judge_lotml(task: ConcurrencyTask, code: str, lotml: Lotml) -> Judged:
    """The answer, after the helpers it does not declare itself, checked and run on the tests."""
    missing = [h for h in task.helper_names if not re.search(rf"^fn {h}\(", code, re.MULTILINE)]
    helpers = "\n".join(
        block
        for block in task.lotml_helpers.split("\n\n")
        if any(f"fn {h}(" in block for h in missing)
    )
    program = (helpers + "\n\n" if helpers else "") + code
    verdict = lotml.judge(task.task, program)
    body = "\n".join(line.split("#", 1)[0] for line in code.splitlines())
    concurrent = "parallel(" in body
    mistake = None
    if verdict.outcome == "does not check":
        if "error[E0112]" in verdict.feedback:
            mistake = "async or await"
        elif "`parallel` takes" in verdict.feedback:
            mistake = "tasks misused"
    return Judged(verdict.passed, concurrent, mistake, verdict.feedback[:500])


# The experiment -------------------------------------------------------------------------------

PYTHON_INSTRUCTIONS = (
    "Complete the async Python function below. The helper coroutines are already defined: call "
    "them concurrently, not one after another. Answer with the function, and any import it needs, "
    "in one ```python code block, with no explanation."
)
LOTML_INSTRUCTIONS = (
    "Complete the lotml function below. The helper functions are already defined: call them "
    "concurrently, not one after another. Answer with the function in one ```lotml code block, "
    "with no explanation."
)


def prompt(task: ConcurrencyTask, language: str) -> tuple[str, str]:
    if language == "python":
        system = "You write Python 3 with type hints. Write only Python."
        code = f"{task.python_helpers}\n\n{task.python_signature}"
        return system, f"{PYTHON_INSTRUCTIONS}\n\n```python\n{code}```\n"
    system, _ = variants.prompt(task.task, "b")
    code = f"{task.lotml_helpers}\n{task.lotml_signature}"
    return system, f"{LOTML_INSTRUCTIONS}\n\n```lotml\n{code}```\n"


def answer(model: Model, task: ConcurrencyTask, language: str, lotml: Lotml) -> dict:
    record = {"model": model.name, "family": model.family, "task": task.id, "language": language}
    system, user = prompt(task, language)
    try:
        completion = model.chat(system, [{"role": "user", "content": user}])
    except ModelError as error:
        return record | {"error": str(error)}
    code = variants.extract(completion.text)
    judged = judge_python(task, code) if language == "python" else judge_lotml(task, code, lotml)
    return record | {
        "error": None,
        "code": code,
        "passed": judged.passed,
        "concurrent": judged.concurrent,
        "mistake": judged.mistake,
        "detail": judged.detail,
    }


def run(model: Model, found: list[ConcurrencyTask], path: Path, workers: int = 1) -> list[dict]:
    lotml = Lotml()
    done = {}
    if path.exists():
        for line in path.read_text(encoding="utf-8").splitlines():
            record = json.loads(line)
            if record.get("error") is None:
                done[(record["task"], record["language"])] = record
    todo = [(t, lang) for t in found for lang in LANGUAGES if (t.id, lang) not in done]
    path.parent.mkdir(parents=True, exist_ok=True)
    with ThreadPoolExecutor(max_workers=workers) as pool:
        for record in pool.map(lambda job: answer(model, job[0], job[1], lotml), todo):
            with path.open("a", encoding="utf-8") as out:
                out.write(json.dumps(record, ensure_ascii=False) + "\n")
            if record.get("error") is None:
                done[(record["task"], record["language"])] = record
    return list(done.values())


def summarize(rows: list[dict]) -> dict[str, dict]:
    """Per model: answers right, concurrent, and with a concurrency mistake, in each language,
    and the paired test on the mistakes."""
    by_model: dict[str, dict[tuple[str, str], dict]] = {}
    for r in rows:
        if r.get("error") is None:
            by_model.setdefault(r["model"], {})[(r["task"], r["language"])] = r
    summary = {}
    for model, records in sorted(by_model.items()):
        paired = sorted({t for t, _ in records if all((t, lang) in records for lang in LANGUAGES)})
        mistaken = {
            lang: {t for t in paired if records[(t, lang)]["mistake"]} for lang in LANGUAGES
        }
        only_python = len(mistaken["python"] - mistaken["lotml"])
        only_lotml = len(mistaken["lotml"] - mistaken["python"])
        summary[model] = {
            "pairs": len(paired),
            "passed": {
                lang: sum(records[(t, lang)]["passed"] for t in paired) for lang in LANGUAGES
            },
            "concurrent": {
                lang: sum(records[(t, lang)]["concurrent"] for t in paired) for lang in LANGUAGES
            },
            "mistakes": {
                lang: dict(Counter(records[(t, lang)]["mistake"] for t in mistaken[lang]))
                for lang in LANGUAGES
            },
            "only_python_mistake": only_python,
            "only_lotml_mistake": only_lotml,
            "p": variants.mcnemar(only_python, only_lotml),
        }
    return summary


def markdown(summary: dict[str, dict]) -> str:
    lines = [
        "# The forgotten-`await` hypothesis",
        "",
        "Generated by `python -m lotml_harness.experiments.awaits`. Each model wrote 30",
        "concurrency tasks once in each language: with Python's asyncio, given helper coroutines,",
        "and in lotml, given helper functions and `parallel`. A Python answer's mistake is a",
        "forgotten `await` (a coroutine never awaited, or used as its value) or colour (`await`",
        "outside an `async def`, a loop started inside the running one); a lotml answer's is",
        "`async` or `await` written anyway, or calls given to `parallel` where tasks are wanted.",
        "Concurrent means the answer used `asyncio.gather` or another way to run the calls at",
        "once, or `parallel`.",
        "",
        "| model | tasks | right, Python | right, lotml | concurrent, Python | concurrent, lotml |"
        " mistakes, Python | mistakes, lotml | only Python | only lotml | McNemar p |",
        "| --- | ---: | ---: | ---: | ---: | ---: | --- | --- | ---: | ---: | ---: |",
    ]

    def counts(found: dict[str, int]) -> str:
        return ", ".join(f"{k} {v}" for k, v in sorted(found.items())) or "none"

    for model, s in summary.items():
        lines.append(
            f"| {model} | {s['pairs']} | {s['passed']['python']} | {s['passed']['lotml']} |"
            f" {s['concurrent']['python']} | {s['concurrent']['lotml']} |"
            f" {counts(s['mistakes']['python'])} | {counts(s['mistakes']['lotml'])} |"
            f" {s['only_python_mistake']} | {s['only_lotml_mistake']} | {s['p']:.3f} |"
        )
    pooled_python = sum(s["only_python_mistake"] for s in summary.values())
    pooled_lotml = sum(s["only_lotml_mistake"] for s in summary.values())
    lines += [
        "",
        f"Pooled: {pooled_python} tasks with a concurrency mistake only in Python,"
        f" {pooled_lotml} only in lotml"
        f" (McNemar p = {variants.mcnemar(pooled_python, pooled_lotml):.3f}).",
        "",
    ]
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--model", action="append", default=[], help="claude:haiku")
    parser.add_argument("--workers", type=int, default=4)
    args = parser.parse_args()
    found = tasks()
    for spec in args.model:
        model = from_spec(spec)
        run(model, found, RUNS / f"{model.name}.jsonl", workers=args.workers)
    rows = [
        json.loads(line)
        for path in sorted(RUNS.glob("*.jsonl"))
        for line in path.read_text(encoding="utf-8").splitlines()
    ]
    report = markdown(summarize(rows))
    REPORT.write_text(report, encoding="utf-8")
    sys.stdout.buffer.write(report.encode("utf-8"))


if __name__ == "__main__":
    main()
