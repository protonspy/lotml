"""Grading a workspace: `lotml check` over it, then each graded file with its hidden tests appended,
through `lotml test --json`. lotml has no imports between a project's own files, so a hidden test
cannot sit in a file of its own; it is appended, in a scratch copy, as phase 1 judges an answer."""

import json
import re
import shutil
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

from lotml_harness.agent.bench import AgentTask
from lotml_harness.agent.mcp import scrub_interfaces
from lotml_harness.experiments.phase1 import Lotml

HIDDEN = re.compile(r'^test "hidden:', re.MULTILINE)


@dataclass
class Grade:
    checks: bool
    passed: int
    total: int
    failures: list[str] = field(default_factory=list)
    """Fixed text — file, hidden test, outcome — safe in the committed rows."""
    details: list[str] = field(default_factory=list)
    """The compiler's output when tests did not run: the agent's text, kept to the trace."""
    python: dict | None = None
    """The CPython the hidden tests ran on, as `lotml test --json` records it (adr:0026): its
    path and exact version, and uv's version."""

    @property
    def failure(self) -> str | None:
        """Why a run that did not pass failed: `signature` when its files check alone but the
        hidden blocks appended do not — the agent's types refused the tests' values — and
        `behaviour` otherwise; None for a pass (specs/agent-humaneval/ R2.5)."""
        if self.outcome == "pass":
            return None
        refused = any(f.endswith("the tests did not run") for f in self.failures)
        return "signature" if self.checks and refused else "behaviour"

    @property
    def outcome(self) -> str:
        if self.total > 0 and self.passed == self.total:
            return "pass"
        return "fail" if self.checks else "no check"


def grade(task: AgentTask, workspace: Path, lotml: Lotml | None = None) -> Grade:
    lotml = lotml or Lotml()
    with tempfile.TemporaryDirectory(prefix="lotml-agent-grade-") as scratch:
        copy = Path(scratch)
        shutil.copytree(workspace, copy, dirs_exist_ok=True, symlinks=True)
        # The agent's code runs below: no interface may reach Python or C for it.
        scrub_interfaces(copy)
        for link in [p for p in copy.rglob("*") if p.is_symlink()]:
            link.unlink()
        checked = lotml.compiler(["check", "."], scratch)
        checks = checked is not None and checked.returncode == 0
        passed, total, failures, details, python = 0, 0, [], [], None
        for file in task.graded:
            hidden = task.hidden(file)
            total += len(HIDDEN.findall(hidden))
            source = copy / file
            if not source.is_file():
                failures.append(f"{file} is missing")
                continue
            # A test the agent named `hidden:` itself is renamed, so only the task's count.
            text = HIDDEN.sub('test "agent:', source.read_text(encoding="utf-8"))
            source.write_text(text.rstrip("\n") + "\n\n" + hidden, encoding="utf-8")
            ran = lotml.compiler(["test", "--json", file], scratch)
            report = _report(ran.stdout if ran else "")
            if report is None:
                failures.append(f"{file}: the tests did not run")
                details.append((ran.stdout + ran.stderr)[-2000:] if ran else "timed out")
                continue
            python = report.get("python", python)
            for test in report["tests"]:
                if not test["name"].startswith("hidden:"):
                    continue
                if test["outcome"] == "pass":
                    passed += 1
                else:
                    failures.append(f"{file}: {test['name']}: {test['outcome']}")
        return Grade(checks, passed, total, failures, details, python)


def _report(stdout: str) -> dict | None:
    """The JSON report `lotml test --json` prints last, or None when it printed none."""
    for line in reversed(stdout.strip().splitlines()):
        try:
            report = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(report, dict) and "tests" in report:
            return report
    return None
