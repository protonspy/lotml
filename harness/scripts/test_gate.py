"""Run the harness suite with coverage and print the report `scc check` reads.

Prints `{"total": N, "coverage": P}` and exits with pytest's own status.
"""

import json
import sys
import tempfile
from pathlib import Path

import pytest


class Counter:
    def __init__(self) -> None:
        self.total = 0

    def pytest_runtest_logreport(self, report) -> None:
        if report.when == "call" or (report.when == "setup" and report.skipped):
            self.total += 1


def main() -> int:
    counter = Counter()
    with tempfile.TemporaryDirectory() as scratch:
        report = Path(scratch) / "coverage.json"
        status = pytest.main(
            [
                "-q",
                "--cov=lotml_harness",
                f"--cov-report=json:{report}",
                *sys.argv[1:],
            ],
            plugins=[counter],
        )
        coverage = json.loads(report.read_text())["totals"]["percent_covered"]
    print(json.dumps({"total": counter.total, "coverage": round(coverage, 1)}))
    return int(status)


if __name__ == "__main__":
    sys.exit(main())
