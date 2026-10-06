"""LiveCodeBench problems as lotml tasks.

A LeetCode problem becomes a function typed by its starter code; an AtCoder or Codeforces
problem becomes `solve(input: str) -> str`, compared line by line with the expected output.
"""

import ast
import base64
import io
import json
import pickle
import textwrap
import zlib
from typing import Any

from lotml_harness.tasks import Case, Task, types, values
from lotml_harness.tasks.extract import Untranslatable

MAX_CASE_CHARS = 20_000
"""A case whose input and output exceed this is a stress test, not a hidden test."""
MAX_CASES = 20

STDIN_NOTE = (
    "Your function `solve` receives the whole standard input as `input` and must "
    "return what the program would print."
)


class PlainData(pickle.Unpickler):
    """An unpickler that refuses every class: the private tests are a pickled str."""

    def find_class(self, module: str, name: str) -> Any:
        raise pickle.UnpicklingError(f"refused {module}.{name}")


PRIVATE_LIMIT = 512 * 2**20
"""Bytes one record's private tests may decompress to: a bound on a decompression bomb."""


def decode_private(encoded: str, limit: int = PRIVATE_LIMIT) -> Any:
    inflater = zlib.decompressobj()
    raw = inflater.decompress(base64.b64decode(encoded.encode()), limit)
    if inflater.unconsumed_tail:
        raise pickle.UnpicklingError(f"private tests decompress past {limit} bytes")
    text = PlainData(io.BytesIO(raw)).load()
    if not isinstance(text, str):
        raise pickle.UnpicklingError("private tests are not a JSON string")
    return json.loads(text)


def docstring(statement: str, stdin: bool) -> str:
    text = statement.strip() + ("\n\n" + STDIN_NOTE if stdin else "")
    text = text.replace("\\", "\\\\").replace('"""', '\\"\\"\\"')
    return "\n" + textwrap.indent(text, "    ", lambda _line: True) + "\n    "


def leetcode_signature(starter: str, name: str) -> tuple[list[tuple[str, types.Type]], types.Type]:
    try:
        module = ast.parse(starter.rstrip() + "\n        pass\n")
    except SyntaxError:
        raise Untranslatable("starter code does not parse") from None
    methods = [
        node for node in ast.walk(module) if isinstance(node, ast.FunctionDef) and node.name == name
    ]
    if len(methods) != 1:
        raise Untranslatable(f"no single method `{name}` in the starter code")
    arguments = methods[0].args.args[1:]
    try:
        if methods[0].returns is None or any(a.annotation is None for a in arguments):
            raise Untranslatable("an untyped signature")
        params = [(a.arg, types.from_annotation(a.annotation)) for a in arguments]
        returns = types.from_annotation(methods[0].returns)
    except types.UnsupportedType as error:
        raise Untranslatable(f"signature: {error}") from None
    if returns == types.Unit():
        raise Untranslatable("a method returning nothing has no result to test")
    return params, returns


def functional_case(raw: dict, task: Task) -> Case:
    try:
        args = [json.loads(line) for line in raw["input"].split("\n") if line.strip()]
        expected = json.loads(raw["output"])
    except json.JSONDecodeError:
        raise Untranslatable("test value: not JSON") from None
    if len(args) != len(task.params):
        raise Untranslatable("test value: wrong number of arguments")
    try:
        conformed = [values.conform(a, t) for a, (_, t) in zip(args, task.params, strict=True)]
        return Case(conformed, values.conform(expected, task.returns))
    except values.Mismatch as error:
        raise Untranslatable(f"test value: {error}") from None


def from_record(record: dict) -> Task:
    """The task for one LiveCodeBench record, or `Untranslatable`."""
    metadata = json.loads(record.get("metadata") or "{}")
    name = metadata.get("func_name")
    stdin = not record.get("starter_code", "").strip()
    if stdin:
        params: list[tuple[str, types.Type]] = [("input", types.Prim("str"))]
        returns: types.Type = types.Prim("str")
        name = "solve"
    elif name is None:
        raise Untranslatable("no function name for the starter code")
    else:
        params, returns = leetcode_signature(record["starter_code"], name)
    task = Task(
        id=f"livecodebench/{record['question_id']}",
        source="livecodebench",
        name=name,
        params=params,
        returns=returns,
        doc=docstring(record["question_content"], stdin),
        tests=[],
        meta={
            "platform": record["platform"],
            "difficulty": record["difficulty"],
            "date": record["contest_date"][:10],
        },
    )
    raw_cases = json.loads(record["public_test_cases"]) + decode_private(
        record["private_test_cases"]
    )
    for raw in raw_cases:
        if len(raw["input"]) + len(raw["output"]) > MAX_CASE_CHARS:
            continue
        if stdin:
            task.tests.append(Case([raw["input"]], raw["output"], "lines"))
        else:
            task.tests.append(functional_case(raw, task))
        if len(task.tests) == MAX_CASES:
            break
    if not task.tests:
        raise Untranslatable("no test of a reasonable size")
    return task
