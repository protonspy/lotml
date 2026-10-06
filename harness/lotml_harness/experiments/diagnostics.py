"""Terse against detailed diagnostics: does the detail of the compiler's report change whether a
model repairs its program? (docs/wiki/pages/semantic-compiler.md, "Detail is not monotonic").

Every lotml first answer the compiler refused in the phase 1 runs is checked again and answered
twice, in conversations alike but for the report. The terse report gives each diagnostic's
location, code and message, with the alternatives in scope, which the design keeps in both
modes. The detailed report is the full one: the source line, labels, notes and fixes, then the
explanation page of every code it shows. The model's next answer is run on the hidden tests.
Paired per answer, the two modes are compared by the exact McNemar test.

    python -m lotml_harness.experiments.diagnostics --model claude:haiku --model ollama:llama3.1:8b
"""

import argparse
import json
import sys
import tempfile
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

from lotml_harness.experiments import phase1, variants
from lotml_harness.experiments.models import Model, ModelError, from_spec
from lotml_harness.experiments.phase1 import RESULTS
from lotml_harness.tasks import Task, build

RUNS = RESULTS / "diagnostics"
REPORT = RESULTS / "diagnostics.md"
MODES = ("terse", "detailed")


def terse(check: dict) -> str:
    """Each diagnostic as one line — where, which code, what — and the alternatives in scope."""
    lines = []
    for d in check["diagnostics"]:
        where = d["location"]
        place = f"{d['file']}:{where['line']}:{where['column']}"
        lines.append(f"{place}: {d['severity']}[{d['code']}]: {d['message']}")
        if d.get("alternatives"):
            lines.append(f"  alternatives: {', '.join(d['alternatives'])}")
    return "\n".join(lines) + "\n"


def detailed(shown: str, check: dict, pages: dict[str, str]) -> str:
    """The full report as `lotml check` prints it, then each code's explanation page, once."""
    codes = list(dict.fromkeys(d["code"] for d in check["diagnostics"]))
    explained = "\n\n".join(pages[c] for c in codes if c in pages)
    return shown.rstrip("\n") + "\n\n" + explained + "\n" if explained else shown


def refused(rows: list[dict]) -> list[tuple[str, str, str]]:
    """The lotml first answers the compiler refused: (model, task, code)."""
    return [
        (r["model"], r["task"], r["rounds"][0]["code"])
        for r in rows
        if r["language"] == "lotml"
        and r.get("error") is None
        and r["rounds"]
        and r["rounds"][0]["outcome"] == "does not check"
    ]


class Compiler:
    """The compiler as the experiment asks it: a report in JSON and as text, a code's page, and
    the judge of an answer."""

    def __init__(self):
        self.lotml = phase1.Lotml()
        self.pages: dict[str, str] = {}

    def check(self, code: str) -> tuple[dict, str] | None:
        """The report on `code`, as JSON and as text; None when the compiler accepts it."""
        with tempfile.TemporaryDirectory(prefix="lotml-diagnostics-") as directory:
            Path(directory, "solution.lotml").write_text(code, encoding="utf-8")
            as_json = self.lotml.compiler(["check", "--json", "solution.lotml"], directory)
            as_text = self.lotml.compiler(["check", "solution.lotml"], directory)
        if as_json is None or as_text is None or as_json.returncode == 0:
            return None
        return json.loads(as_json.stdout), as_text.stdout

    def explain(self, code: str) -> str:
        if code not in self.pages:
            with tempfile.TemporaryDirectory() as directory:
                found = self.lotml.compiler(["explain", code], directory)
            self.pages[code] = found.stdout.strip() if found is not None else ""
        return self.pages[code]

    def judge(self, task: Task, code: str) -> phase1.Verdict:
        return self.lotml.judge(task, code)


def ask(model: Model, task: Task, code: str, mode: str, compiler: Compiler) -> dict | None:
    """The refused answer `code` answered with the report in `mode`, and how the next answer
    did; None when the compiler now accepts `code`, so there is nothing to report."""
    checked = compiler.check(code)
    if checked is None:
        return None
    report, shown = checked
    if mode == "terse":
        body = terse(report)
    else:
        codes = {d["code"] for d in report["diagnostics"]}
        body = detailed(shown, report, {c: compiler.explain(c) for c in codes})
    system, user = variants.prompt(task, "b")
    messages = [
        {"role": "user", "content": user},
        {"role": "assistant", "content": f"```lotml\n{code}```"},
        {
            "role": "user",
            "content": f"`lotml check` reports:\n\n{body}\nAnswer with the corrected code.",
        },
    ]
    completion = model.chat(system, messages)
    answer = variants.extract(completion.text)
    verdict = compiler.judge(task, answer)
    return {
        "model": model.name,
        "task": task.id,
        "mode": mode,
        "error": None,
        "codes": [d["code"] for d in report["diagnostics"]],
        "passed": verdict.passed,
        "outcome": verdict.outcome,
        "answer": answer,
        "input_tokens": completion.input_tokens,
        "output_tokens": completion.output_tokens,
    }


def run(model: Model, answers: list[tuple[Task, str]], path: Path, workers: int = 1) -> list[dict]:
    """Each refused answer in both modes, resuming from `path`."""
    compiler = Compiler()
    done = {}
    if path.exists():
        for line in path.read_text(encoding="utf-8").splitlines():
            record = json.loads(line)
            if record.get("error") is None:
                done[(record["task"], record["mode"])] = record
    todo = [(t, c, m) for t, c in answers for m in MODES if (t.id, m) not in done]

    def one(job):
        task, code, mode = job
        try:
            record = ask(model, task, code, mode, compiler)
        except ModelError as error:
            return {"model": model.name, "task": task.id, "mode": mode, "error": str(error)}
        return record or {
            "model": model.name,
            "task": task.id,
            "mode": mode,
            "error": None,
            "accepted": True,
        }

    path.parent.mkdir(parents=True, exist_ok=True)
    with ThreadPoolExecutor(max_workers=workers) as pool:
        for record in pool.map(one, todo):
            with path.open("a", encoding="utf-8") as out:
                out.write(json.dumps(record, ensure_ascii=False) + "\n")
            if record.get("error") is None:
                done[(record["task"], record["mode"])] = record
    return list(done.values())


def summarize(records: list[dict]) -> dict[str, dict]:
    """Per model: the answers asked in both modes, how many each mode repaired, and the test."""
    by_model: dict[str, dict[str, dict[str, bool]]] = {}
    for r in records:
        if r.get("error") is None and "passed" in r:
            by_model.setdefault(r["model"], {}).setdefault(r["task"], {})[r["mode"]] = r["passed"]
    summary = {}
    for model, tasks in sorted(by_model.items()):
        paired = {t: m for t, m in tasks.items() if all(mode in m for mode in MODES)}
        only_terse = sum(m["terse"] and not m["detailed"] for m in paired.values())
        only_detailed = sum(m["detailed"] and not m["terse"] for m in paired.values())
        summary[model] = {
            "pairs": len(paired),
            "solved": {mode: sum(m[mode] for m in paired.values()) for mode in MODES},
            "only_terse": only_terse,
            "only_detailed": only_detailed,
            "p": variants.mcnemar(only_terse, only_detailed),
        }
    return summary


def markdown(summary: dict[str, dict]) -> str:
    lines = [
        "# Terse against detailed diagnostics",
        "",
        "Generated by `python -m lotml_harness.experiments.diagnostics`. Each lotml first answer",
        "the compiler refused in the phase 1 runs was checked again by the current compiler and",
        "answered twice: with the terse report (each diagnostic's location, code and message, and",
        "the alternatives in scope) and with the detailed one (the full report — source line,",
        "labels, notes, fixes — and each code's explanation page). Repaired means the next answer",
        "passed the hidden tests. An answer the current compiler accepts is left out.",
        "",
        "| model | answers | repaired, terse | repaired, detailed | only terse | only detailed |"
        " McNemar p |",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    totals = {"pairs": 0, "terse": 0, "detailed": 0, "only_terse": 0, "only_detailed": 0}
    for model, s in summary.items():
        n = max(1, s["pairs"])
        lines.append(
            f"| {model} | {s['pairs']} | {s['solved']['terse']} ({s['solved']['terse'] / n:.0%}) |"
            f" {s['solved']['detailed']} ({s['solved']['detailed'] / n:.0%}) | {s['only_terse']} |"
            f" {s['only_detailed']} | {s['p']:.3f} |"
        )
        totals["pairs"] += s["pairs"]
        totals["terse"] += s["solved"]["terse"]
        totals["detailed"] += s["solved"]["detailed"]
        totals["only_terse"] += s["only_terse"]
        totals["only_detailed"] += s["only_detailed"]
    p = variants.mcnemar(totals["only_terse"], totals["only_detailed"])
    lines.append(
        f"| pooled | {totals['pairs']} | {totals['terse']} | {totals['detailed']} |"
        f" {totals['only_terse']} | {totals['only_detailed']} | {p:.3f} |"
    )
    lines.append("")
    return "\n".join(lines)


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--model", action="append", default=[], help="claude:haiku")
    parser.add_argument("--workers", type=int, default=4)
    args = parser.parse_args()
    tasks = {t.id: t for t in build.load()}
    for spec in args.model:
        model = from_spec(spec)
        rows_path = phase1.RUNS / f"{model.name}.jsonl"
        rows = [json.loads(line) for line in rows_path.read_text(encoding="utf-8").splitlines()]
        answers = [
            (tasks[t], code) for m, t, code in refused(rows) if m == model.name and t in tasks
        ]
        run(model, answers, RUNS / f"{model.name}.jsonl", workers=args.workers)
    records = [
        json.loads(line)
        for path in sorted(RUNS.glob("*.jsonl"))
        for line in path.read_text(encoding="utf-8").splitlines()
    ]
    report = markdown(summarize(records))
    REPORT.write_text(report, encoding="utf-8")
    sys.stdout.buffer.write(report.encode("utf-8"))


if __name__ == "__main__":
    main()
