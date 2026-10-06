"""Run the harness and the compiler suites with coverage and print the report `scc check` reads.

Prints `{"total": N, "coverage": P}`: the tests of both suites, and the share of lines covered
across both — the harness's Python measured by coverage.py, the compiler's Rust by
`cargo llvm-cov`. Exits non-zero if either suite fails.
"""

import json
import subprocess
import sys
import tempfile
from pathlib import Path

import pytest

COMPILER = Path(__file__).resolve().parents[2] / "compiler"


class Counter:
    def __init__(self) -> None:
        self.total = 0

    def pytest_runtest_logreport(self, report) -> None:
        if report.when == "call" or (report.when == "setup" and report.skipped):
            self.total += 1


def harness() -> tuple[int, int, int, int]:
    """Pytest's status, its tests, and the lines covered and coverable."""
    counter = Counter()
    with tempfile.TemporaryDirectory() as scratch:
        report = Path(scratch) / "coverage.json"
        status = pytest.main(
            ["-q", "--cov=lotml_harness", f"--cov-report=json:{report}", *sys.argv[1:]],
            plugins=[counter],
        )
        totals = json.loads(report.read_text())["totals"]
    return int(status), counter.total, totals["covered_lines"], totals["num_statements"]


def compiler() -> tuple[int, int, int, int]:
    """`cargo llvm-cov`'s status, the tests it ran, and the lines covered and coverable."""
    with tempfile.TemporaryDirectory() as scratch:
        report = Path(scratch) / "coverage.json"
        run = subprocess.run(  # noqa: S603
            [  # noqa: S607 - cargo from PATH, as the developer runs it
                "cargo",
                "llvm-cov",
                "--workspace",
                "--json",
                "--summary-only",
                "--output-path",
                str(report),
            ],
            cwd=COMPILER,
            capture_output=True,
            text=True,
            encoding="utf-8",
            check=False,
        )
        sys.stdout.write(run.stdout[-2000:])
        sys.stderr.write(run.stderr[-4000:])
        if run.returncode != 0 or not report.exists():
            return run.returncode or 1, 0, 0, 0
        lines = json.loads(report.read_text())["data"][0]["totals"]["lines"]
    # Each test binary ends with `test result: ok. 22 passed; 0 failed; …`.
    tests = sum(
        int(line.split(" passed")[0].rsplit(" ", 1)[-1])
        for line in (run.stdout + run.stderr).splitlines()
        if line.startswith("test result:")
    )
    return 0, tests, lines["covered"], lines["count"]


def main() -> int:
    py_status, py_tests, py_covered, py_lines = harness()
    rs_status, rs_tests, rs_covered, rs_lines = compiler()
    coverage = 100 * (py_covered + rs_covered) / max(1, py_lines + rs_lines)
    print(json.dumps({"total": py_tests + rs_tests, "coverage": round(coverage, 1)}))
    return py_status or rs_status


if __name__ == "__main__":
    sys.exit(main())
