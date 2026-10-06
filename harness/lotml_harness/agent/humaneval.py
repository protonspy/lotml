"""HumanEval's and MBPP's original releases as agent tasks, posed without types.

The originals are read at pinned commits through a digested download into the git-ignored cache;
nothing of them is committed (adr:0015-pose-humaneval-untyped, specs/agent-humaneval/).

Run as `python -m lotml_harness.agent.humaneval`, this module is the child process that records a
problem's cases: it reads a request as JSON on stdin and prints one result line.
"""

import ast
import contextlib
import copy
import gzip
import io
import json
import math
import random
import secrets
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from typing import Any

from lotml_harness.compare import TOLERANCE
from lotml_harness.execute import MEMORY, child_environment, limit_memory
from lotml_harness.tasks import sources, types, values

DECOMPRESSED_LIMIT = 64 * 2**20
"""Bytes HumanEval's file may decompress to: it is some 200 kB."""
CASES = 50
"""Cases kept per problem, the first ones made."""
RECORD_SECONDS = 60
"""Wall clock for recording one problem's cases."""
RECORD_OUTPUT = 4 * 2**20
"""Bytes of the recorder's output the parent reads; a problem whose cases need more is refused."""


def humaneval_url() -> str:
    return (
        f"https://raw.githubusercontent.com/openai/human-eval/{sources.HUMAN_EVAL}"
        "/data/HumanEval.jsonl.gz"
    )


def read_humaneval() -> tuple[list[dict], str]:
    """HumanEval's records and the SHA-256 of the bytes they were parsed from."""
    target = sources.CACHE / "human-eval" / sources.HUMAN_EVAL / "HumanEval.jsonl.gz"
    data = sources.pinned(humaneval_url(), target, sources.HUMAN_EVAL_SHA256)
    with gzip.GzipFile(fileobj=io.BytesIO(data)) as packed:
        text = packed.read(DECOMPRESSED_LIMIT + 1)
    if len(text) > DECOMPRESSED_LIMIT:
        raise ValueError(f"{target.name} decompresses past {DECOMPRESSED_LIMIT} bytes")
    records = [json.loads(line) for line in text.decode("utf-8").splitlines() if line.strip()]
    return records, sources.HUMAN_EVAL_SHA256


NO_LITERAL = object()
"""A recorded value `repr` wrote as something that is not a Python literal: an object."""


@dataclass
class Recorded:
    """A problem's cases as the canonical solution answered them, or why there are none."""

    cases: list[tuple[tuple, Any]] = field(default_factory=list)
    error: str | None = None


def record(
    code: str, test: str, entry: str, style: str, setup: str = "", seconds: float = RECORD_SECONDS
) -> Recorded:
    """Run `test` against the canonical `code` in a child process and keep each call's arguments
    and result. `style` is "check" (HumanEval: `check(candidate)`) or "asserts" (MBPP: lines
    calling `entry` by name). The child gets the clean environment, an empty working directory, a
    memory cap and a wall clock, and only `RECORD_OUTPUT` bytes of what it prints are read."""
    nonce = secrets.token_hex(16)
    request = {
        "code": code,
        "test": test,
        "entry": entry,
        "style": style,
        "setup": setup,
        "limit": CASES,
        "memory": MEMORY,
        "nonce": nonce,
    }
    with (
        tempfile.TemporaryDirectory(prefix="lotml-record-") as empty,
        tempfile.TemporaryFile() as out,
        tempfile.TemporaryFile() as err,
    ):
        try:
            subprocess.run(  # noqa: S603
                [sys.executable, "-m", "lotml_harness.agent.humaneval"],
                input=json.dumps(request).encode("utf-8"),
                stdout=out,
                stderr=err,
                cwd=empty,
                timeout=seconds,
                env=child_environment(),
                check=False,
            )
        except subprocess.TimeoutExpired:
            return Recorded(error="timeout")
        out.seek(0)
        output = out.read(RECORD_OUTPUT + 1)
        if len(output) > RECORD_OUTPUT:
            return Recorded(error="output")
        err.seek(0)
        errors = err.read(RECORD_OUTPUT).decode("utf-8", "replace")
    for line in reversed(output.decode("utf-8", "replace").splitlines()):
        if line.startswith(nonce):
            answer = json.loads(line[len(nonce) :])
            if answer["error"] is not None:
                return Recorded(error=answer["error"])
            return Recorded(cases=[(_literal(a), _literal(r)) for a, r in answer["cases"]])
    return Recorded(error="crash: " + (errors.strip().splitlines() or ["no result"])[-1])


def _literal(text: str) -> Any:
    try:
        return ast.literal_eval(text)
    except (ValueError, TypeError, SyntaxError, MemoryError, RecursionError):
        return NO_LITERAL


class Recorder:
    """Stands in for the canonical function: calls it, and keeps the outermost calls' arguments,
    copied before the call since some solutions change them, and results, as `repr` text."""

    def __init__(self, function, limit: int):
        self.function = function
        self.limit = limit
        self.depth = 0
        self.cases: list[tuple[str, str]] = []
        self.seen: set[tuple[str, str]] = set()

    def __call__(self, *args, **keywords):
        if keywords:
            raise TypeError("the test calls the function with keyword arguments")
        if self.depth:
            return self.function(*args)
        arguments = repr(copy.deepcopy(args))
        self.depth += 1
        try:
            result = self.function(*args)
        finally:
            self.depth -= 1
        case = (arguments, repr(result))
        if case not in self.seen and len(self.cases) < self.limit:
            self.seen.add(case)
            self.cases.append(case)
        return result


class Discard(io.TextIOBase):
    """Where the dataset's own printing goes."""

    def write(self, text: str) -> int:
        return len(text)


def run_request(request: dict) -> dict:
    """The child's work: the cases `request`'s test makes of its code, or the error it hit."""
    namespace: dict = {"__name__": "__record__"}
    try:
        with contextlib.redirect_stdout(Discard()):
            exec(compile(request["code"], "<code>", "exec"), namespace)  # noqa: S102
            exec(compile(request["setup"], "<setup>", "exec"), namespace)  # noqa: S102
            recorder = Recorder(namespace[request["entry"]], request["limit"])
            random.seed(0)
            if request["style"] == "check":
                exec(compile(request["test"], "<test>", "exec"), namespace)  # noqa: S102
                namespace["check"](recorder)
            else:
                namespace[request["entry"]] = recorder
                exec(compile(request["test"], "<test>", "exec"), namespace)  # noqa: S102
    except Exception as failure:  # noqa: BLE001 - any failure of dataset code is a refusal
        return {"cases": [], "error": f"test: {type(failure).__name__}"}
    return {"cases": recorder.cases, "error": None}


class NoLiteral(ValueError):
    """A recorded value, or a column of them, that no lotml literal of one type can write."""


@dataclass(frozen=True)
class _Unknown:
    """The items of an empty container: whatever the rest of the column says."""


def infer(value: Any) -> types.Type:
    """The lotml type of one value, by its Python type; `_Unknown` for an empty container's
    items."""
    if value is NO_LITERAL:
        raise NoLiteral("an object has no lotml literal")
    if isinstance(value, bool):
        return types.Prim("bool")
    if isinstance(value, int):
        if not values.I64_MIN <= value <= values.I64_MAX:
            raise NoLiteral(f"{value} overflows int")
        return types.Prim("int")
    if isinstance(value, float):
        if not math.isfinite(value):
            raise NoLiteral(f"{value!r} has no lotml literal")
        return types.Prim("f64")
    if isinstance(value, str):
        return types.Prim("str")
    if value is None:
        return types.Unit()
    if isinstance(value, list):
        return types.List(_join_all(infer(v) for v in value))
    if isinstance(value, tuple):
        if not value:
            raise NoLiteral("the empty tuple has no lotml literal")
        return types.Tuple(tuple(infer(v) for v in value))
    if isinstance(value, set | frozenset):
        return types.Set(_join_all(infer(v) for v in value))
    if isinstance(value, dict):
        return types.Dict(
            _join_all(infer(k) for k in value), _join_all(infer(v) for v in value.values())
        )
    raise NoLiteral(f"a {type(value).__name__} has no lotml literal")


def _join_all(found) -> types.Type:
    joined: types.Type = _Unknown()
    for type_ in found:
        joined = join(joined, type_)
    return joined


def join(a: types.Type, b: types.Type) -> types.Type:
    """The one type whose literals write both `a`'s values and `b`'s: numbers widen to `f64`,
    `None` makes a type optional, containers join their items. Any other pair is refused."""
    if a == b:
        return a
    if isinstance(a, _Unknown):
        return b
    if isinstance(b, _Unknown):
        return a
    if isinstance(a, types.Unit):
        return b if isinstance(b, types.Optional) else types.Optional(b)
    if isinstance(b, types.Unit):
        return join(b, a)
    if isinstance(a, types.Optional) or isinstance(b, types.Optional):
        inner_a = a.inner if isinstance(a, types.Optional) else a
        inner_b = b.inner if isinstance(b, types.Optional) else b
        return types.Optional(join(inner_a, inner_b))
    match a, b:
        case (types.Prim("int"), types.Prim("f64")) | (types.Prim("f64"), types.Prim("int")):
            return types.Prim("f64")
        case types.List(x), types.List(y):
            return types.List(join(x, y))
        case types.Set(x), types.Set(y):
            return types.Set(join(x, y))
        case types.Dict(k1, v1), types.Dict(k2, v2):
            return types.Dict(join(k1, k2), join(v1, v2))
        case types.Tuple(xs), types.Tuple(ys) if len(xs) == len(ys):
            return types.Tuple(tuple(join(x, y) for x, y in zip(xs, ys, strict=True)))
    raise NoLiteral(f"{_shown(a)} and {_shown(b)} have no common lotml type")


def _shown(type_: types.Type) -> str:
    return "an empty container's item" if isinstance(type_, _Unknown) else type_.render("b")


def _settle(type_: types.Type) -> types.Type:
    """`type_` with an empty container's items as `int`, refusing `None` that is never optional."""
    match type_:
        case _Unknown():
            return types.Prim("int")
        case types.Unit():
            raise NoLiteral("a value that is always None has no lotml literal")
        case types.Optional(inner):
            return types.Optional(_settle(inner))
        case types.List(item):
            return types.List(_settle(item))
        case types.Set(item):
            return types.Set(_settle(item))
        case types.Dict(key, item):
            return types.Dict(_settle(key), _settle(item))
        case types.Tuple(items):
            return types.Tuple(tuple(_settle(t) for t in items))
    return type_


def column(found) -> types.Type:
    """The one lotml type of a parameter's, or the result's, values across a problem's cases."""
    return _settle(_join_all(infer(v) for v in found))


def literal(value: Any, type_: types.Type) -> str:
    """`value` as a lotml literal of `type_`, the type its column was given."""
    try:
        return values.render(values.conform(value, type_), type_, "b")
    except values.Mismatch as mismatch:
        raise NoLiteral(str(mismatch)) from mismatch


def _holds_float(type_: types.Type) -> bool:
    match type_:
        case types.Prim("f64"):
            return True
        case types.Optional(inner) | types.List(inner) | types.Set(inner):
            return _holds_float(inner)
        case types.Dict(key, item):
            return _holds_float(key) or _holds_float(item)
        case types.Tuple(items):
            return any(_holds_float(t) for t in items)
    return False


def _assertions(expression: str, value: Any, type_: types.Type, fresh) -> list[tuple[int, str]]:
    """The lines asserting `expression` is `value`: within the tolerance where a float sits in a
    list, a tuple or an optional, exactly elsewhere. Each line carries its extra indentation."""
    if value is None:
        return [(0, f"assert {expression} is None")]
    if not _holds_float(type_):
        return [(0, f"assert {expression} == {literal(value, type_)}")]
    match type_:
        case types.Prim("f64"):
            shown = literal(value, type_)
            bound = f"{TOLERANCE!r} * max(1.0, abs({shown}))"
            return [(0, f"assert abs({expression} - {shown}) <= {bound}")]
        case types.Optional(inner):
            name = fresh()
            inside = _assertions(name, value, inner, fresh)
            return [
                (0, f"{name} = {expression}"),
                (0, f"assert {name} is not None"),
                (0, f"if {name} is not None:"),
                *((depth + 1, line) for depth, line in inside),
            ]
        case types.List(item):
            name = fresh()
            lines = [(0, f"{name} = {expression}"), (0, f"assert len({name}) == {len(value)}")]
            for index, element in enumerate(value):
                lines += _assertions(f"{name}[{index}]", element, item, fresh)
            return lines
        case types.Tuple(items) if len(items) > 1:
            names = [fresh() for _ in items]
            lines = [(0, f"{', '.join(names)} = {expression}")]
            for name, element, item in zip(names, value, items, strict=True):
                lines += _assertions(name, element, item, fresh)
            return lines
    return [(0, f"assert {expression} == {literal(value, type_)}")]


def hidden_blocks(
    entry: str, cases: list[tuple[tuple, Any]], params: list[types.Type], returns: types.Type
) -> str:
    """One `test "hidden: <n>"` block per case, calling `entry` with the case's arguments."""
    blocks = []
    for number, (arguments, expected) in enumerate(cases, 1):
        written = ", ".join(literal(a, t) for a, t in zip(arguments, params, strict=True))
        count = iter(range(1_000_000))
        lines = _assertions(
            f"{entry}({written})", expected, returns, lambda count=count: f"r{next(count)}"
        )
        body = "".join("    " * (1 + depth) + line + "\n" for depth, line in lines)
        blocks.append(f'test "hidden: {number}":\n{body}')
    return "\n".join(blocks)


def main() -> None:
    request = json.load(sys.stdin)
    nonce = request.pop("nonce")
    limit_memory(request.pop("memory"))
    sys.setrecursionlimit(10_000)
    answer = run_request(request)
    sys.stdout.write("\n" + nonce + json.dumps(answer) + "\n")


if __name__ == "__main__":
    main()
