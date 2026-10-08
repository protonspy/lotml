"""Run a program: its `test` blocks, or a task's hidden tests, under a step budget.

The program is lotml run by the phase 0 transpiler (`mode` "lotml", or "python" for the same
program read with Python's semantics), lotml compiled by `lotml build --target python`
("compiled": the module stub the compiler wrote), or a model's typed Python ("solution").

`run` executes in this process and is what the child process calls; `isolated` starts that
child, so a program that hangs past the step budget's reach, exhausts memory or kills its
interpreter takes only the child with it. Model-written code is still not sandboxed —
run unreviewed batches on a machine you can throw away.
"""

import builtins
import json
import linecache
import os
import secrets
import subprocess
import sys
import tempfile
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
from lotml_harness.limits import limit_memory
from lotml_harness.tasks import Task, values

BUDGET = 10_000_000
"""Lines one test may run: a deterministic stand-in for a time limit."""
TOOL = 4
"""The `sys.monitoring` tool id the step budget uses (0 to 2 and 5 are reserved)."""
COMPILER_RUNTIME = ROOT / "compiler" / "crates" / "lotml-py" / "runtime"
"""Where the compiler's Python runtime, `lotml_rt`, lives."""
SOLUTION_MODULES = frozenset(
    {
        "typing",
        "math",
        "collections",
        "itertools",
        "functools",
        "heapq",
        "bisect",
        "re",
        "string",
        "dataclasses",
        "enum",
        "fractions",
        "decimal",
        "statistics",
        "operator",
        "copy",
        "abc",
        "__future__",
    }
)
"""The modules a Python solution may import. A denylist and an import list are a convenience that
keeps an honest answer to the common modules, not a boundary: Python introspection
(`().__class__`, a traceback's frames, `typing.sys`) walks around both. The boundary is the
machine — see the module docstring."""
BLOCKED_BUILTINS = frozenset(
    {"open", "eval", "exec", "compile", "input", "breakpoint", "help", "exit", "quit"}
)


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
    (BaseException, "runtime error"),
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
    observed: dict[str, str] = field(default_factory=dict)
    """What the function returned on each hidden test it got wrong, as a literal."""

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


def compiler_runtime():
    """`lotml_rt`, the runtime modules compiled by `lotml build --target python` import."""
    if str(COMPILER_RUNTIME) not in sys.path:
        sys.path.insert(0, str(COMPILER_RUNTIME))
    import lotml_rt

    return lotml_rt


def solution_import(name, globals_=None, locals_=None, fromlist=(), level=0):
    if name.split(".")[0] not in SOLUTION_MODULES or level != 0:
        raise ImportError(f"import of {name} is outside the modules a solution may use")
    return builtins.__import__(name, globals_, locals_, fromlist, level)


def load_compiled(stub: str):
    """A module compiled by `lotml build --target python`, run from the stub the compiler wrote."""
    lotml_rt = compiler_runtime()
    try:
        path, payload = lotml_rt.stub_arguments(stub)
    except (SyntaxError, ValueError) as error:
        return Result(error=f"load: {error}")
    namespace = types.ModuleType("lotml_program").__dict__
    prelude = {**lotml_rt.PRELUDE, "print": runtime.capped_print()}
    try:
        code = lotml_rt.load(namespace, path, payload, prelude)
    except BaseException as error:  # noqa: BLE001
        return Result(error=f"load: {type(error).__name__}: {error}")
    return namespace, code


def load_solution(source: str, path: str):
    """A model's typed Python, with the common builtins and modules of computation. This narrows
    an honest answer, not a hostile one: a model's Python is native code, and like every mode it
    runs on a machine you can throw away (see the module docstring), not inside a sandbox."""
    try:
        code = compile(source, path, "exec")
    except SyntaxError as error:
        return Result(error=f"parse: {error.msg} (line {error.lineno})")
    linecache.cache[path] = (len(source), None, source.splitlines(True), path)
    allowed = {n: getattr(builtins, n) for n in dir(builtins) if n not in BLOCKED_BUILTINS}
    prelude = {**allowed, "print": runtime.capped_print(), "__import__": solution_import}
    namespace = {"__builtins__": prelude, "__name__": "solution"}
    try:
        exec(code, namespace)  # noqa: S102
    except BaseException as error:  # noqa: BLE001
        return Result(error=f"load: {type(error).__name__}: {error}")
    return namespace, code


def load(source: str, variant: str, mode: str, path: str):
    """The program's namespace and code, or a `Result` saying why it never ran."""
    if mode == "compiled":
        return load_compiled(source)
    if mode == "solution":
        return load_solution(source, path)
    try:
        tree = transpile(source, variant, mode)
    except LarkError as error:
        return Result(error=f"parse: {str(error).strip().splitlines()[0]}")
    except (TranspileError, RecursionError) as error:
        return Result(error=f"transpile: {error}")
    linecache.cache[path] = (len(source), None, source.splitlines(True), path)
    namespace = types.ModuleType("lotml_program").__dict__
    prelude = {
        **runtime.PRELUDE,
        "print": runtime.capped_print(),
        "__import__": restricted_import,
    }
    # Every name the transpiler generates starts with `__`, which a program cannot write.
    namespace.update(
        {
            "__rt": runtime,
            "__variants": runtime.Variants(),
            "__tests": [],
            "__builtins__": prelude,
        }
    )
    try:
        code = compile(tree, path, "exec")
    except (ValueError, TypeError, SyntaxError) as error:
        return Result(error=f"transpile: {error}")
    try:
        exec(code, namespace)  # noqa: S102
    except BaseException as error:  # noqa: BLE001
        return Result(error=f"load: {type(error).__name__}: {error}")
    return namespace, code


def attempt(budget: Budget, limit: int, function, *args) -> tuple[str | None, Any, BaseException]:
    """`function(*args)` under the budget: (None, result, None) or (label, None, error).

    `BaseException` is caught too: a program that leaves through `SystemExit` has failed, not
    finished.
    """
    budget.left = limit
    try:
        return None, function(*args), None
    except BaseException as error:  # noqa: BLE001
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
            for name, test in namespace["__tests"]:
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
                if outcome == "wrong answer":
                    result.observed[str(index)] = literal(value, task, mode, variant)
            result.cases.append(outcome)
            if error is not None and str(index) not in result.tracebacks:
                result.tracebacks[str(index)] = report(error, path)
    return result


def literal(value: Any, task: Task, mode: str, variant: str) -> str:
    """A returned value as the program's language writes it."""
    try:
        conformed = values.conform(value, task.returns)
    except values.Mismatch:
        return repr(value)
    if mode == "solution":
        return repr(conformed)
    return values.render(conformed, task.returns, variant)


MEMORY = 2 * 2**30
"""Bytes a child may use."""
OUTPUT_TAIL = 1_000_000
"""Bytes of a child's output the parent reads: the end, where the result line is."""
ENVIRONMENT = (
    "PATH",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "WINDIR",
    "TEMP",
    "TMP",
    "TMPDIR",
    "LANG",
    "LOTML_PYTHON",
)
"""What a child keeps of the environment; `LOTML_PYTHON` chooses the CPython the Python target runs
on, so a whole run can be held to one version (plans/python-via-uv.md 1.1)."""


def child_environment() -> dict[str, str]:
    """What a child needs to start Python and nothing else: no tokens, no keys. The compiler a
    child runs gets an interpreter already resolved, the harness's own unless `LOTML_PYTHON` names
    one, so it never reaches the download step (adr:0026)."""
    kept = {k: v for k, v in os.environ.items() if k.upper() in ENVIRONMENT}
    kept.setdefault("LOTML_PYTHON", sys.executable)
    return kept | {
        "PYTHONPATH": str(ROOT / "harness"),
        "PYTHONIOENCODING": "utf-8",
        "PYTHONDONTWRITEBYTECODE": "1",
    }


def tail(file) -> str:
    file.seek(0, os.SEEK_END)
    file.seek(max(0, file.tell() - OUTPUT_TAIL))
    return file.read().decode("utf-8", "replace")


def isolated(
    source: str,
    variant: str = "b",
    mode: str = "lotml",
    task: Task | None = None,
    budget: int = BUDGET,
    timeout: float = 120,
    memory: int = MEMORY,
) -> Result:
    """`run` in a child process with a minimal environment, a memory cap and a wall clock.

    The child marks its result line with a nonce the program never sees, so a program
    printing a result of its own is not believed; output goes to a file and only its end is
    read, so a program printing without end cannot fill the parent's memory.
    """
    nonce = secrets.token_hex(16)
    request = {
        "source": source,
        "variant": variant,
        "mode": mode,
        "task": task.to_json() if task is not None else None,
        "budget": budget,
        "memory": memory,
        "nonce": nonce,
    }
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        try:
            subprocess.run(  # noqa: S603
                [sys.executable, "-m", "lotml_harness.execute"],
                input=json.dumps(request).encode("utf-8"),
                stdout=out,
                stderr=err,
                timeout=timeout,
                env=child_environment(),
                check=False,
            )
        except subprocess.TimeoutExpired:
            return Result(error="timeout: the program ran past the wall-clock limit")
        output, errors = tail(out), tail(err)
    for line in reversed(output.splitlines()):
        if line.startswith(nonce):
            return Result(**json.loads(line[len(nonce) :]))
    reason = (errors.strip().splitlines() or ["no result"])[-1]
    return Result(error=f"crash: {reason}")


def main() -> None:
    request = json.load(sys.stdin)
    nonce = request.pop("nonce")
    limit_memory(request.pop("memory"))
    task = Task.from_json(request["task"]) if request["task"] is not None else None
    sys.setrecursionlimit(10_000)
    result = run(request["source"], request["variant"], request["mode"], task, request["budget"])
    sys.stdout.write("\n" + nonce + json.dumps(asdict(result)) + "\n")


if __name__ == "__main__":
    main()
