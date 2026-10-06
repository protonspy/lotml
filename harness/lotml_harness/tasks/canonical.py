"""Run a task's canonical Python solution on its hidden tests, in a child process.

Reads a task as JSON on stdin and prints one verdict per test case as a JSON list. The
canonical solutions come from the datasets, not from a model, but a child process still
keeps a hang or a crash from taking the build with it.
"""

import json
import sys

from lotml_harness.compare import matches
from lotml_harness.execute import MEMORY, limit_memory
from lotml_harness.tasks import Task


def verdicts(task: Task) -> list[bool]:
    namespace: dict = {}
    exec(compile(task.canonical, f"<{task.id}>", "exec"), namespace)  # noqa: S102
    function = namespace[task.name]
    results = []
    for case in task.tests:
        try:
            results.append(matches(function(*case.args), case, task.returns))
        except Exception:  # noqa: BLE001
            results.append(False)
    return results


def main() -> None:
    limit_memory(MEMORY)
    task = Task.from_json(json.load(sys.stdin))
    print(json.dumps(verdicts(task)))


if __name__ == "__main__":
    main()
