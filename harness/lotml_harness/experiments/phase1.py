"""The phase 1 gate: lotml with its compiler against typed Python, on the same tasks.

Each model writes every sampled task in lotml, from the reference, and in typed Python. A lotml
answer is checked by `lotml check` and, when it checks, compiled by `lotml build` and run on the
task's hidden tests; a Python answer is run on the same tests. After a failed answer the model
sees what a developer would — the compiler's diagnostics, the failing tests with the value its
function returned, or the error that stopped it — and answers again, up to `ROUNDS` answers in
all. Reported per model: pass@1 in each language with the exact McNemar test, the median number
of answers to a green run, and the tokens of the programs that pass.

The gate (plans/lotml-roadmap.md, task 2.13): lotml's pass@1 is not below typed Python's, the
median rounds to green is at most 2, and lotml's passing programs take no more tokens than
Python's.

    python -m lotml_harness.experiments.phase1 --model claude:haiku --model ollama:qwen2.5-coder:7b
"""

import argparse
import json
import os
import re
import statistics
import subprocess
import tempfile
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path

from lotml_harness import ROOT
from lotml_harness.execute import Result, child_environment, isolated
from lotml_harness.experiments import variants
from lotml_harness.experiments.models import Model, ModelError, from_spec
from lotml_harness.tasks import Task, build, types, values

RESULTS = ROOT / "harness" / "results"
RUNS = RESULTS / "phase1"
REPORT = RESULTS / "phase1.md"
ROUNDS = 3
"""Answers per task: the first, and two after feedback."""
COMPILER_TIMEOUT = 60
"""Seconds the checker or the backend may take on one answer before it is given up on."""
LANGUAGES = ("lotml", "python")
FAILURES_SHOWN = 3
"""Failing hidden tests a feedback message describes: small reports repair better."""
MEDIAN_ROUNDS = 2
PAIRS_NEEDED = 168
ALPHA = 0.05
BINARY = "lotml.exe" if os.name == "nt" else "lotml"
COMPILER = Path(os.environ.get("LOTML_BIN", ROOT / "compiler" / "target" / "release" / BINARY))
"""The compiler: `cargo build --release -p lotml` in `compiler/`, or `LOTML_BIN`."""

PYTHON_INSTRUCTIONS = (
    "Complete the Python function below. Answer with the whole function, and any helper it "
    "needs, in one ```python code block, with type hints and no explanation."
)
PYTHON_NAMES = {"int": "int", "f64": "float", "str": "str", "bool": "bool"}


def python_type(t: types.Type) -> str:
    match t:
        case types.Prim(name):
            return PYTHON_NAMES.get(name, name)
        case types.Unit():
            return "None"
        case types.Optional(inner):
            return f"{python_type(inner)} | None"
        case types.List(item):
            return f"list[{python_type(item)}]"
        case types.Set(item):
            return f"set[{python_type(item)}]"
        case types.Dict(key, value):
            return f"dict[{python_type(key)}, {python_type(value)}]"
        case types.Tuple(items):
            return f"tuple[{', '.join(python_type(i) for i in items)}]"
    raise ValueError(f"no Python type for {t!r}")


def python_prompt(task: Task) -> tuple[str, str]:
    params = ", ".join(f"{n}: {python_type(t)}" for n, t in task.params)
    head = f"def {task.name}({params}) -> {python_type(task.returns)}:\n"
    doc = f'    """{task.doc}"""\n' if task.doc else ""
    system = "You write Python 3 with type hints. Write only Python."
    return system, f"{PYTHON_INSTRUCTIONS}\n\n```python\n{head}{doc}```\n"


@dataclass
class Verdict:
    passed: bool
    outcome: str
    """`pass`, `does not check`, `tests fail`, or `does not run`."""
    feedback: str


def render(value, t: types.Type, language: str) -> str:
    try:
        conformed = values.conform(value, t)
    except values.Mismatch:
        return repr(value)
    if language == "python":
        return repr(conformed)
    return values.render(conformed, t, "b")


def test_feedback(task: Task, result: Result, language: str) -> str:
    """The failing hidden tests, each with what the function did."""
    lines = []
    for index, outcome in enumerate(result.cases):
        if outcome == "pass":
            continue
        case = task.tests[index]
        pairs = zip(case.args, task.params, strict=True)
        args = ", ".join(render(a, t, language) for a, (_, t) in pairs)
        call = f"{task.name}({args})"
        expected = render(case.expected, task.returns, language)
        if outcome == "wrong answer":
            observed = result.observed.get(str(index), "?")
            lines.append(f"- `{call}` returned `{observed}`; expected `{expected}`.")
        else:
            trace = result.tracebacks.get(str(index), outcome).strip().splitlines()
            lines.append(f"- `{call}` stopped: {trace[-1] if trace else outcome}.")
        if len(lines) == FAILURES_SHOWN:
            break
    failed = sum(o != "pass" for o in result.cases)
    head = f"{failed} of {len(result.cases)} hidden tests fail:"
    return head + "\n" + "\n".join(lines)


class Lotml:
    """The lotml arm: `lotml check`, then the compiled program on the hidden tests."""

    def __init__(self, binary: Path = COMPILER):
        self.binary = binary

    def prompt(self, task: Task) -> tuple[str, str]:
        return variants.prompt(task, "b")

    def compiler(self, args: list[str], directory: str) -> subprocess.CompletedProcess | None:
        """Run the compiler on model code in `directory`: a clean environment so it inherits no
        token, a wall clock so a pathological input cannot stall the run. None on timeout."""
        try:
            return subprocess.run(  # noqa: S603
                [str(self.binary), *args],
                cwd=directory,
                capture_output=True,
                text=True,
                encoding="utf-8",
                env=child_environment(),
                timeout=COMPILER_TIMEOUT,
                check=False,
            )
        except subprocess.TimeoutExpired:
            return None

    def judge(self, task: Task, code: str) -> Verdict:
        with tempfile.TemporaryDirectory(prefix="lotml-phase1-") as directory:
            source = Path(directory) / "solution.lotml"
            source.write_text(code, encoding="utf-8")
            # Run in the directory with a relative path, so the report names `solution.lotml`
            # as the model knows it, not the scratch directory.
            check = self.compiler(["check", source.name], directory)
            if check is None:
                return Verdict(False, "does not check", "`lotml check` did not finish in time.")
            if check.returncode != 0:
                return Verdict(False, "does not check", "`lotml check` reports:\n\n" + check.stdout)
            built = self.compiler(["build", "-o", ".", source.name], directory)
            stub = Path(directory) / "solution_lotml.py"
            if built is None or built.returncode != 0 or not stub.exists():
                report = (built.stdout + built.stderr) if built else "build timed out"
                return Verdict(False, "does not run", report)
            result = isolated(stub.read_text(encoding="utf-8"), mode="compiled", task=task)
        return verdict(task, result, "lotml")


class Python:
    """The typed Python arm: the answer run on the hidden tests."""

    def prompt(self, task: Task) -> tuple[str, str]:
        return python_prompt(task)

    def judge(self, task: Task, code: str) -> Verdict:
        return verdict(task, isolated(code, mode="solution", task=task), "python")


def verdict(task: Task, result: Result, language: str) -> Verdict:
    if result.error is not None:
        return Verdict(False, "does not run", result.error)
    if result.passed:
        return Verdict(True, "pass", "")
    return Verdict(False, "tests fail", test_feedback(task, result, language))


def converse(model: Model, arm, task: Task, language: str) -> dict:
    """Up to `ROUNDS` answers, each after the feedback on the one before."""
    system, user = arm.prompt(task)
    messages = [{"role": "user", "content": user}]
    record = {"model": model.name, "family": model.family, "language": language, "task": task.id}
    rounds = []
    for _ in range(ROUNDS):
        try:
            completion = model.chat(system, messages)
        except ModelError as error:
            return record | {"error": str(error), "rounds": rounds}
        code = variants.extract(completion.text)
        judged = arm.judge(task, code)
        rounds.append(
            {
                "code": code,
                "outcome": judged.outcome,
                "feedback": judged.feedback,
                "input_tokens": completion.input_tokens,
                "output_tokens": completion.output_tokens,
                "seconds": round(completion.seconds, 2),
            }
        )
        if judged.passed:
            break
        messages += [
            {"role": "assistant", "content": completion.text},
            {"role": "user", "content": judged.feedback + "\n\nAnswer with the corrected code."},
        ]
    green = next((i + 1 for i, r in enumerate(rounds) if r["outcome"] == "pass"), None)
    return record | {"error": None, "rounds": rounds, "green": green}


def run(model: Model, tasks: list[Task], path: Path, workers: int = 1) -> list[dict]:
    """Every task in both languages, resuming from `path`."""
    arms = {"lotml": Lotml(), "python": Python()}
    done = {}
    if path.exists():
        for line in path.read_text(encoding="utf-8").splitlines():
            record = json.loads(line)
            if record.get("error") is None:
                done[(record["task"], record["language"])] = record
    todo = [(t, lang) for t in tasks for lang in LANGUAGES if (t.id, lang) not in done]
    path.parent.mkdir(parents=True, exist_ok=True)
    with ThreadPoolExecutor(max_workers=workers) as pool:
        jobs = pool.map(lambda job: converse(model, arms[job[1]], *job), todo)
        for record in jobs:
            with path.open("a", encoding="utf-8") as out:
                out.write(json.dumps(record, ensure_ascii=False) + "\n")
            if record.get("error") is None:
                done[(record["task"], record["language"])] = record
    return [done[(t.id, lang)] for t in tasks for lang in LANGUAGES if (t.id, lang) in done]


def program_tokens(code: str) -> int:
    """A program's size in tokens, by the open `o200k_base` encoding: free, deterministic, and
    within one boundary token of Claude's count on the paired corpus (results/tokens.md)."""
    import tiktoken

    return len(tiktoken.get_encoding("o200k_base").encode(code))


def summarize(rows: list[dict]) -> dict[str, dict]:
    """Per model: pass@1 in each language, the paired comparison, rounds and tokens."""
    by_model: dict[str, dict[tuple[str, str], dict]] = {}
    for row in rows:
        by_model.setdefault(row["model"], {})[(row["task"], row["language"])] = row
    summary = {}
    for model, records in sorted(by_model.items()):
        tasks = sorted({task for task, _ in records})
        paired = [t for t in tasks if (t, "lotml") in records and (t, "python") in records]
        first = {k: r["green"] == 1 for k, r in records.items()}
        only_lotml = sum(first[(t, "lotml")] and not first[(t, "python")] for t in paired)
        only_python = sum(first[(t, "python")] and not first[(t, "lotml")] for t in paired)
        greens = [r["green"] for (_, lang), r in records.items() if lang == "lotml" and r["green"]]
        ratios = []
        for t in paired:
            lotml, python = records[(t, "lotml")], records[(t, "python")]
            if lotml["green"] and python["green"]:
                size = program_tokens(lotml["rounds"][lotml["green"] - 1]["code"])
                ratios.append(
                    size / max(1, program_tokens(python["rounds"][python["green"] - 1]["code"]))
                )
        summary[model] = {
            "family": next(iter(records.values()))["family"],
            "pairs": len(paired),
            "pass1": {
                lang: sum(first[(t, lang)] for t in paired) / max(1, len(paired))
                for lang in LANGUAGES
            },
            "solved": {
                lang: sum(bool(records[(t, lang)]["green"]) for t in paired) / max(1, len(paired))
                for lang in LANGUAGES
            },
            "only_lotml": only_lotml,
            "only_python": only_python,
            "p": variants.mcnemar(only_lotml, only_python),
            "median_rounds": statistics.median(greens) if greens else None,
            "token_ratio": statistics.median(ratios) if ratios else None,
            "output_tokens": {
                lang: sum(r["output_tokens"] for t in paired for r in records[(t, lang)]["rounds"])
                for lang in LANGUAGES
            },
            "first": {
                lang: Counter(records[(t, lang)]["rounds"][0]["outcome"] for t in paired)
                for lang in LANGUAGES
            },
            "codes": Counter(
                code
                for t in paired
                if (code := first_code(records[(t, "lotml")]["rounds"][0])) is not None
            ).most_common(),
        }
    return summary


OUTCOMES = ("pass", "does not check", "tests fail", "does not run")
CODE = re.compile(r"error\[(E\d{4})\]")


def first_code(answer: dict) -> str | None:
    """The first error code `lotml check` gave an answer it refused, if it refused it."""
    if answer["outcome"] != "does not check":
        return None
    found = CODE.search(answer.get("feedback") or "")
    return found.group(1) if found else None


@dataclass
class Criterion:
    name: str
    passed: bool
    evidence: str


def gate(summary: dict[str, dict]) -> list[Criterion]:
    """The phase 1 gate on the summary: each criterion with what decided it."""
    enough = all(s["pairs"] >= PAIRS_NEEDED for s in summary.values()) and bool(summary)
    worse = [m for m, s in summary.items() if s["only_python"] > s["only_lotml"] and s["p"] < ALPHA]
    pooled_lotml = sum(s["only_lotml"] for s in summary.values())
    pooled_python = sum(s["only_python"] for s in summary.values())
    pass1 = "; ".join(
        f"{m}: {s['pairs']} pairs, lotml {s['pass1']['lotml']:.1%},"
        f" Python {s['pass1']['python']:.1%},"
        f" {s['only_lotml']} only lotml, {s['only_python']} only Python, p = {s['p']:.3f}"
        for m, s in summary.items()
    )
    rounds = {m: s["median_rounds"] for m, s in summary.items()}
    ratios = [s["token_ratio"] for s in summary.values() if s["token_ratio"] is not None]
    pooled_ratio = statistics.median(ratios) if ratios else None
    return [
        Criterion(
            "lotml's pass@1 is not below typed Python's",
            enough and not worse and pooled_lotml >= pooled_python,
            f"{pass1}; pooled: {pooled_lotml} only lotml, {pooled_python} only Python",
        ),
        Criterion(
            f"the median rounds to green is at most {MEDIAN_ROUNDS}",
            bool(rounds) and all(r is not None and r <= MEDIAN_ROUNDS for r in rounds.values()),
            "; ".join(f"{m}: {r}" for m, r in rounds.items()),
        ),
        Criterion(
            "lotml's passing programs take no more tokens than Python's",
            pooled_ratio is not None and pooled_ratio <= 1,
            "; ".join(
                f"{m}: {s['token_ratio']:.2f}" for m, s in summary.items() if s["token_ratio"]
            )
            + (f"; median of models: {pooled_ratio:.2f}" if pooled_ratio is not None else ""),
        ),
    ]


def markdown(summary: dict[str, dict], criteria: list[Criterion]) -> str:
    lines = [
        "# Phase 1 gate: lotml with its compiler against typed Python",
        "",
        "Generated by `python -m lotml_harness.experiments.phase1`. Each model writes each task in",
        f"both languages, with up to {ROUNDS} answers: the first, and the rest after feedback —",
        "`lotml check`'s diagnostics, the failing hidden tests with the value returned, or the",
        "error that stopped the program. Pass@1 is the first answer's; program tokens count the",
        "passing programs with `o200k_base`, as a ratio of lotml to Python per task. A",
        "conversation that a model call broke off stays in the log with its error and is not",
        "counted; the task is asked again on the next run.",
        "",
        "| model | family | pairs | lotml pass@1 | Python pass@1 | lotml solved | Python solved |"
        " only lotml | only Python | McNemar p | median rounds | tokens, lotml/Python |",
        "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    for model, s in summary.items():
        ratio = f"{s['token_ratio']:.2f}" if s["token_ratio"] is not None else "—"
        lines.append(
            f"| {model} | {s['family']} | {s['pairs']} | {s['pass1']['lotml']:.1%} |"
            f" {s['pass1']['python']:.1%} | {s['solved']['lotml']:.1%} |"
            f" {s['solved']['python']:.1%} | {s['only_lotml']} | {s['only_python']} |"
            f" {s['p']:.3f} | {s['median_rounds']} | {ratio} |"
        )
    lines += [
        "",
        "How each first answer ended, and the first error of each lotml answer the compiler",
        "refused (`lotml explain <code>` says what it means):",
        "",
        "| model | language | " + " | ".join(OUTCOMES) + " | first errors |",
        "| --- | --- | " + " | ".join("---:" for _ in OUTCOMES) + " | --- |",
    ]
    for model, s in summary.items():
        for lang in LANGUAGES:
            counts = " | ".join(str(s["first"][lang][o]) for o in OUTCOMES)
            codes = ", ".join(f"{c} {n}" for c, n in s["codes"]) if lang == "lotml" else ""
            lines.append(f"| {model} | {lang} | {counts} | {codes or '—'} |")
    lines += ["", "| criterion | result | evidence |", "| --- | --- | --- |"]
    for c in criteria:
        lines.append(f"| {c.name} | {'pass' if c.passed else 'fail'} | {c.evidence} |")
    verdict = "passes" if all(c.passed for c in criteria) else "does not pass"
    lines += ["", f"The gate {verdict}.", ""]
    return "\n".join(lines)


def collected(directory: Path, tasks: list[Task]) -> list[dict]:
    wanted = {t.id for t in tasks}
    rows = []
    for path in sorted(directory.glob("*.jsonl")):
        found = {}
        for line in path.read_text(encoding="utf-8").splitlines():
            record = json.loads(line)
            if record.get("error") is None and record["task"] in wanted:
                found[(record["task"], record["language"])] = record
        rows += found.values()
    return rows


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument(
        "--model", action="append", default=[], help="claude:haiku, ollama:qwen2.5-coder:7b"
    )
    parser.add_argument("--workers", type=int, default=4)
    args = parser.parse_args()
    tasks = variants.sample(build.load(), variants.SAMPLE)
    for spec in args.model:
        model = from_spec(spec)
        run(model, tasks, RUNS / f"{model.name}.jsonl", workers=args.workers)
    summary = summarize(collected(RUNS, tasks))
    REPORT.write_text(markdown(summary, gate(summary)), encoding="utf-8")
    print(REPORT.read_text(encoding="utf-8"))


if __name__ == "__main__":
    main()
