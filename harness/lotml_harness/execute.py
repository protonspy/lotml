"""Run a lotml program: its `test` blocks, or a task's hidden tests, under a step budget.

`run` executes in this process and is what the child process calls; `isolated` starts that
child, so a program that hangs past the step budget's reach, exhausts memory or kills its
interpreter takes only the child with it. Model-written code is still not sandboxed —
run unreviewed batches on a machine you can throw away.
"""

import builtins
import json
import linecache
import os
import subprocess
import sys
import traceback
import types
from dataclasses import asdict, dataclass, field
from typing import Any

from lark.exceptions import LarkError

from lotml_harness import ROOT
from lotml_harness.compare import matches
from lotml_harness.lang import runtime
from lotml_harness.lang.check import violations
from lotml_harness.lang.transpile import ALLOWED_MODULES, TranspileError, transpile
from lotml_harness.tasks import Task, values

BUDGET = 10_000_000
"""Lines one test may run: a deterministic stand-in for a time limit."""
TOOL = 4
"""The `sys.monitoring` tool id the step budget uses (0 to 2 and 5 are reserved)."""


class StepBudgetExceeded(Exception):
    """A test ran more lines than its budget."""


OUTCOMES = [
    (StepBudgetExceeded, "timeout"),
    (RecursionError, "timeout"),
    (runtime.Fail, "propagated error"),
    (runtime.NonExhaustiveMatch, "non-exhaustive match"),
    (runtime.Overflow, "overflow"),
    (runtime.Todo, "todo"),
    (runtime.Panic, "panic"),
    (runtime.LotmlTypeError, "type error"),
    (AssertionError, "assertion"),
    (NameError, "unresolved name"),
    (Exception, "runtime error"),
]


@dataclass
class Result:
    """What running a program produced.

    `error` is set when the program never ran: `parse: …`, `transpile: …` or `load: …`.
    `tests` maps each `test` block to its outcome, `cases` lists one outcome per hidden
    test case — `pass`, `wrong answer`, or the label of what stopped it. `violations` are
    the mutability errors lotml's compiler would reject the program for.
    """

    error: str | None = None
    tests: dict[str, str] = field(default_factory=dict)
    cases: list[str] = field(default_factory=list)
    tracebacks: dict[str, str] = field(default_factory=dict)
    violations: list[str] = field(default_factory=list)

    @property
    def passed(self) -> bool:
        outcomes = [*self.tests.values(), *self.cases]
        return (
            self.error is None
            and not self.violations
            and bool(outcomes)
            and all(o == "pass" for o in outcomes)
        )


def restricted_import(name, globals_=None, locals_=None, fromlist=(), level=0):
    if name not in ALLOWED_MODULES or level != 0:
        raise ImportError(f"import of {name} is outside the prelude's modules")
    return builtins.__import__(name, globals_, locals_, fromlist, level)


def code_objects(code: types.CodeType):
    yield code
    for constant in code.co_consts:
        if isinstance(constant, types.CodeType):
            yield from code_objects(constant)


class Budget:
    """Counts the program's own lines with `sys.monitoring`, and stops it past a limit."""

    def __init__(self, code: types.CodeType):
        self.codes = list(code_objects(code))
        self.left = 0

    def line(self, _code, _line):
        self.left -= 1
        if self.left < 0:
            raise StepBudgetExceeded

    def __enter__(self):
        monitoring = sys.monitoring
        monitoring.use_tool_id(TOOL, "lotml-budget")
        monitoring.register_callback(TOOL, monitoring.events.LINE, self.line)
        for code in self.codes:
            monitoring.set_local_events(TOOL, code, monitoring.events.LINE)
        return self

    def __exit__(self, *_):
        monitoring = sys.monitoring
        for code in self.codes:
            monitoring.set_local_events(TOOL, code, 0)
        monitoring.register_callback(TOOL, monitoring.events.LINE, None)
        monitoring.free_tool_id(TOOL)
        return False


def label(error: BaseException) -> str:
    return next(name for cls, name in OUTCOMES if isinstance(error, cls))


def report(error: BaseException, path: str) -> str:
    failure = traceback.TracebackException.from_exception(error)
    frames = [f for f in failure.stack if f.filename == path]
    return "".join(traceback.format_list(frames)) + "".join(failure.format_exception_only())


def load(source: str, variant: str, mode: str, path: str):
    """The program's namespace and code, or a `Result` saying why it never ran."""
    try:
        tree = transpile(source, variant, mode)
    except LarkError as error:
        return Result(error=f"parse: {str(error).strip().splitlines()[0]}")
    except (TranspileError, RecursionError) as error:
        return Result(error=f"transpile: {error}")
    linecache.cache[path] = (len(source), None, source.splitlines(True), path)
    namespace = types.ModuleType("lotml_program").__dict__
    namespace.update(
        _rt=runtime,
        _variants=runtime.Variants(),
        _tests=[],
        __builtins__={**runtime.PRELUDE, "__import__": restricted_import},
    )
    code = compile(tree, path, "exec")
    try:
        exec(code, namespace)  # noqa: S102
    except Exception as error:  # noqa: BLE001
        return Result(error=f"load: {type(error).__name__}: {error}")
    return namespace, code


def attempt(budget: Budget, limit: int, function, *args) -> tuple[str | None, Any, BaseException]:
    """`function(*args)` under the budget: (None, result, None) or (label, None, error)."""
    budget.left = limit
    try:
        return None, function(*args), None
    except Exception as error:  # noqa: BLE001
        return label(error), None, error


def run(
    source: str,
    variant: str = "b",
    mode: str = "lotml",
    task: Task | None = None,
    budget: int = BUDGET,
    path: str = "program.lotml",
) -> Result:
    """Load `source` and run its `test` blocks, or the task's hidden tests when given one."""
    loaded = load(source, variant, mode, path)
    if isinstance(loaded, Result):
        return loaded
    namespace, code = loaded
    result = Result(violations=violations(variant, source) if mode == "lotml" else [])
    with Budget(code) as counter:
        if task is None:
            for name, test in namespace["_tests"]:
                outcome, _, error = attempt(counter, budget, test)
                result.tests[name] = outcome or "pass"
                if error is not None:
                    result.tracebacks[name] = report(error, path)
            return result
        function = namespace.get(task.name)
        if not callable(function):
            result.error = f"load: no function `{task.name}`"
            return result
        for index, case in enumerate(task.tests):
            args = [
                values.from_json(values.to_json(a, t), t)
                for a, (_, t) in zip(case.args, task.params, strict=True)
            ]
            outcome, value, error = attempt(counter, budget, function, *args)
            if outcome is None:
                outcome = "pass" if matches(value, case, task.returns) else "wrong answer"
            result.cases.append(outcome)
            if error is not None and str(index) not in result.tracebacks:
                result.tracebacks[str(index)] = report(error, path)
    return result


def isolated(
    source: str,
    variant: str = "b",
    mode: str = "lotml",
    task: Task | None = None,
    budget: int = BUDGET,
    timeout: float = 120,
) -> Result:
    """`run` in a child process, which a wall-clock timeout can kill."""
    request = {
        "source": source,
        "variant": variant,
        "mode": mode,
        "task": task.to_json() if task is not None else None,
        "budget": budget,
    }
    environment = os.environ | {"PYTHONPATH": str(ROOT / "harness")}
    try:
        child = subprocess.run(  # noqa: S603
            [sys.executable, "-m", "lotml_harness.execute"],
            input=json.dumps(request),
            capture_output=True,
            text=True,
            encoding="utf-8",
            timeout=timeout,
            env=environment,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return Result(error="timeout: the program ran past the wall-clock limit")
    lines = child.stdout.strip().splitlines()
    if child.returncode != 0 or not lines:
        reason = (child.stderr.strip().splitlines() or ["no output"])[-1]
        return Result(error=f"crash: {reason}")
    return Result(**json.loads(lines[-1]))


def main() -> None:
    request = json.load(sys.stdin)
    task = Task.from_json(request["task"]) if request["task"] is not None else None
    sys.setrecursionlimit(10_000)
    result = run(request["source"], request["variant"], request["mode"], task, request["budget"])
    sys.stdout.write("\n" + json.dumps(asdict(result)) + "\n")


if __name__ == "__main__":
    main()
