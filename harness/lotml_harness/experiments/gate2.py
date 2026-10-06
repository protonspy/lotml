"""The phase 2 gate: incremental adoption from Python working, and the corpus validated by tests
(plans/lotml-roadmap.md, task 3.7; docs/wiki/pages/evaluation-harness.md).

- **Python calls lotml.** Every program of the corpus is compiled by `lotml build`, imported by
  Python and its function called the way a Python module calls it — through the checked
  boundary of adr:0012, not as `lotml test` runs it — on its task's hidden tests.
- **lotml calls Python.** `lotml bind` writes the interface of Python's `textwrap` from a stub, a
  lotml program calls it through `lotml run`, and its output is what Python's own `textwrap`
  prints for the same calls, including the `PyError` of a call Python refuses.
- **The corpus is validated by tests.** Every program's own `test` block passes `lotml test`.

    python -m lotml_harness.experiments.gate2
"""

import json
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

from lotml_harness.corpus.pipeline import CORPUS
from lotml_harness.execute import child_environment
from lotml_harness.experiments.phase1 import COMPILER, RESULTS
from lotml_harness.tasks import Task, build

REPORT = RESULTS / "gate-2.md"
TIMEOUT = 600

CALLER = r"""
import importlib, json, sys
sys.path.insert(0, sys.argv[1])
from lotml_harness.compare import matches
from lotml_harness.tasks import Task
results = []
for job in json.loads(sys.stdin.read()):
    task = Task.from_json(job["task"])
    row = {"task": task.id, "passed": 0, "failed": 0, "refused": []}
    try:
        function = getattr(importlib.import_module(job["module"]), task.name)
    except Exception as error:
        row["refused"].append("import: " + type(error).__name__ + ": " + str(error)[:200])
        results.append(row)
        continue
    for case in task.tests:
        try:
            ok = matches(function(*case.args), case, task.returns)
        except (TypeError, OverflowError) as error:
            row["refused"].append(type(error).__name__ + ": " + str(error)[:200])
            ok = False
        except Exception:
            ok = False
        row["passed" if ok else "failed"] += 1
    results.append(row)
print(json.dumps(results))
"""


def compiler(
    args: list[str], directory: Path, timeout: float = TIMEOUT
) -> subprocess.CompletedProcess:
    return subprocess.run(  # noqa: S603 - the compiler, on programs this harness wrote
        [str(COMPILER), *args],
        cwd=directory,
        capture_output=True,
        text=True,
        encoding="utf-8",
        env=child_environment(),
        timeout=timeout,
        check=False,
    )


@dataclass
class Check:
    total: int
    passed: int
    detail: list[str]


def python_calls_lotml(entries: list[dict], tasks: dict[str, Task]) -> Check:
    """Each corpus program compiled, imported by Python and called on its hidden tests."""
    with tempfile.TemporaryDirectory(prefix="lotml-gate2-") as directory:
        root = Path(directory)
        jobs = []
        for i, entry in enumerate(entries):
            (root / f"p{i}.lotml").write_text(entry["lotml"], encoding="utf-8")
            jobs.append({"module": f"p{i}_lotml", "task": tasks[entry["task"]].to_json()})
        built = compiler(
            ["build", "-o", "out", *(f"p{i}.lotml" for i in range(len(entries)))], root
        )
        if built.returncode != 0:
            return Check(len(entries), 0, [built.stdout[:500]])
        called = subprocess.run(  # noqa: S603 - Python, on the modules just built
            [sys.executable, "-c", CALLER, str(root / "out")],
            input=json.dumps(jobs),
            capture_output=True,
            text=True,
            encoding="utf-8",
            env=child_environment(),
            timeout=TIMEOUT,
            check=False,
        )
    try:
        rows = json.loads(called.stdout.strip().splitlines()[-1])
    except (json.JSONDecodeError, IndexError):
        return Check(len(entries), 0, [called.stderr[-500:]])
    failing = [r for r in rows if r["failed"] or r["refused"] or not r["passed"]]
    detail = [f"{r['task']}: {r['failed']} failed; {'; '.join(r['refused'][:2])}" for r in failing]
    return Check(len(entries), len(rows) - len(failing), detail)


TEXTWRAP_PYI = """\
def wrap(text: str, width: int = 70, *, max_lines: int | None = None) -> list[str]: ...
def dedent(text: str) -> str: ...
def shorten(text: str, width: int, *, placeholder: str = ...) -> str: ...
"""

PROGRAM = """\
from textwrap import dedent, wrap, shorten

fn main() -> None ! PyError:
    text = dedent("    lotml calls Python through a checked boundary")?
    for line in wrap(text, width=12)?:
        print(line)
    print(shorten(text, 20)?)
    match shorten(text, -1):
        case Ok(t):
            print(t)
        case Err(e):
            print(e.kind)
"""


def lotml_calls_python() -> Check:
    """A program calling Python's `textwrap` through the interface `lotml bind` wrote, its
    output checked against Python's own."""
    import textwrap

    text = textwrap.dedent("    lotml calls Python through a checked boundary")
    expected = (
        "\n".join([*textwrap.wrap(text, width=12), textwrap.shorten(text, 20), "ValueError"]) + "\n"
    )
    with tempfile.TemporaryDirectory(prefix="lotml-gate2-") as directory:
        root = Path(directory)
        (root / "textwrap.pyi").write_text(TEXTWRAP_PYI, encoding="utf-8")
        (root / "main.lotml").write_text(PROGRAM, encoding="utf-8")
        bound = compiler(["bind", "textwrap", "--stub", "textwrap.pyi"], root)
        if bound.returncode != 0:
            return Check(1, 0, [bound.stderr[:500]])
        ran = compiler(["run", "main.lotml"], root)
    output = ran.stdout.replace("\r\n", "\n")
    if output == expected:
        return Check(1, 1, [])
    return Check(1, 0, [f"printed {output!r}, Python prints {expected!r}; {ran.stderr[:300]}"])


def corpus_tests(entries: list[dict]) -> Check:
    """Each corpus program's own `test` block, run by `lotml test`."""
    tested = [e for e in entries if e["tests"]]
    with tempfile.TemporaryDirectory(prefix="lotml-gate2-") as directory:
        root = Path(directory)
        names = []
        for i, entry in enumerate(tested):
            names.append(f"p{i}.lotml")
            (root / names[-1]).write_text(entry["lotml"], encoding="utf-8")
        ran = compiler(["test", "--json", *names], root)
    try:
        report = json.loads(ran.stdout.strip().splitlines()[-1])
    except (json.JSONDecodeError, IndexError):
        return Check(len(tested), 0, [ran.stdout[:500]])
    failing = sorted({Path(t["file"]).name for t in report["tests"] if t["outcome"] != "pass"})
    detail = [tested[int(name[1:].split(".")[0])]["task"] for name in failing]
    return Check(len(tested), len(tested) - len(failing), detail)


def markdown(adoption: Check, calls: Check, tests: Check) -> str:
    def row(name: str, check: Check, what: str) -> str:
        verdict = "pass" if check.passed == check.total and check.total else "fail"
        evidence = f"{check.passed} of {check.total} {what}"
        if check.detail:
            evidence += "; " + "; ".join(check.detail[:5])
        return f"| {name} | {verdict} | {evidence} |"

    lines = [
        "# Phase 2 gate: incremental adoption from Python, and a corpus validated by tests",
        "",
        "Generated by `python -m lotml_harness.experiments.gate2`. Python calls each corpus",
        "program as a Python module calls it, through the checked boundary (adr:0012); a lotml",
        "program calls Python's `textwrap` through the interface `lotml bind` wrote from a stub,",
        "and prints what Python prints; each corpus program's own `test` block passes",
        "`lotml test`.",
        "",
        "| criterion | result | evidence |",
        "| --- | --- | --- |",
        row(
            "Python calls lotml: every corpus program passes its hidden tests", adoption, "programs"
        ),
        row("lotml calls Python through a generated interface", calls, "programs"),
        row(
            "the corpus is validated by tests: every test block passes",
            tests,
            "programs with tests",
        ),
        "",
    ]
    passed = all(c.passed == c.total and c.total for c in (adoption, calls, tests))
    lines += [f"The gate {'passes' if passed else 'does not pass'}.", ""]
    return "\n".join(lines)


def main() -> None:
    entries = [json.loads(line) for line in CORPUS.read_text(encoding="utf-8").splitlines()]
    tasks = {t.id: t for t in build.load()}
    report = markdown(
        python_calls_lotml(entries, tasks), lotml_calls_python(), corpus_tests(entries)
    )
    REPORT.write_text(report, encoding="utf-8")
    sys.stdout.buffer.write(report.encode("utf-8"))


if __name__ == "__main__":
    main()
