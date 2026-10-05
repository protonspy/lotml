"""MultiPL-E's typed Python originals of HumanEval and MBPP, as lotml tasks.

Each file holds the prompt (imports, one typed function and its docstring), the canonical
solution after `### Canonical solution below ###`, and a `check(candidate)` function after
`### Unit tests below ###`. Every assertion on `candidate` becomes a hidden test case; a
task with an assertion that is not a comparison of literals is refused rather than tested
partially, as are helpers in the prompt and types lotml cannot express.
"""

import ast
import re
from typing import Any

from lotml_harness.tasks import Case, Task, types, values

CANONICAL = "### Canonical solution below ###"
UNIT_TESTS = "### Unit tests below ###"


class Untranslatable(ValueError):
    """A task the harness cannot pose or test faithfully in lotml."""


def literal(node: ast.expr) -> Any:
    """A literal value, including `set()` and `set([...])`, or `Untranslatable`."""
    if (
        isinstance(node, ast.Call)
        and isinstance(node.func, ast.Name)
        and node.func.id == "set"
        and not node.keywords
        and len(node.args) <= 1
    ):
        return set(literal(node.args[0])) if node.args else set()
    try:
        return ast.literal_eval(node)
    except (ValueError, TypeError, SyntaxError, MemoryError, RecursionError):
        raise Untranslatable(f"not a literal: {ast.unparse(node)}") from None


def candidate_call(node: ast.expr) -> list[ast.expr] | None:
    if (
        isinstance(node, ast.Call)
        and isinstance(node.func, ast.Name)
        and node.func.id == "candidate"
    ):
        if node.keywords or any(isinstance(a, ast.Starred) for a in node.args):
            raise Untranslatable("candidate called with keywords or unpacking")
        return node.args
    return None


def set_of_call(node: ast.expr) -> list[ast.expr] | None:
    """The arguments of `candidate` in `set(candidate(...))`."""
    if (
        isinstance(node, ast.Call)
        and isinstance(node.func, ast.Name)
        and node.func.id == "set"
        and len(node.args) == 1
    ):
        return candidate_call(node.args[0])
    return None


FALSY = {
    types.Prim("bool"): False,
    types.Prim("int"): 0,
    types.Prim("f64"): 0.0,
    types.Prim("str"): "",
}


def falsy(returns: types.Type) -> Any:
    if returns in FALSY:
        return FALSY[returns]
    match returns:
        case types.List(_):
            return []
        case types.Dict(_, _):
            return {}
        case types.Set(_):
            return set()
    raise Untranslatable(f"`not` on a {returns.render('b')} has no single value")


def assertion(test: ast.expr, returns: types.Type) -> tuple[list[ast.expr], Any, str]:
    """The candidate's arguments, the expected value and the comparison of one assert."""
    if isinstance(test, ast.Compare) and len(test.ops) == 1:
        left, op, right = test.left, test.ops[0], test.comparators[0]
        if isinstance(op, ast.Eq | ast.Is):
            for call, other in ((left, right), (right, left)):
                if (args := candidate_call(call)) is not None:
                    return args, literal(other), "eq"
                if (args := set_of_call(call)) is not None:
                    return args, sorted(literal(other)), "set"
        if (
            isinstance(op, ast.Lt | ast.LtE)
            and isinstance(left, ast.Call)
            and isinstance(left.func, ast.Name)
            and left.func.id == "abs"
            and len(left.args) == 1
            and isinstance(left.args[0], ast.BinOp)
            and isinstance(left.args[0].op, ast.Sub)
        ):
            difference = left.args[0]
            if (args := candidate_call(difference.left)) is not None:
                return args, literal(difference.right), "approx"
    if (args := candidate_call(test)) is not None:
        if returns != types.Prim("bool"):
            raise Untranslatable("truthiness of a non-bool result")
        return args, True, "eq"
    if (
        isinstance(test, ast.UnaryOp)
        and isinstance(test.op, ast.Not)
        and (args := candidate_call(test.operand)) is not None
    ):
        return args, falsy(returns), "eq"
    raise Untranslatable(f"assertion lotml cannot pose: {ast.unparse(test)}")


def cases(check: ast.FunctionDef, task: Task) -> list[Case]:
    found = []
    for statement in check.body:
        if isinstance(statement, ast.Pass) or (
            isinstance(statement, ast.Expr) and isinstance(statement.value, ast.Constant)
        ):
            continue
        if not isinstance(statement, ast.Assert):
            raise Untranslatable(f"statement in check: {ast.unparse(statement)[:60]}")
        if isinstance(statement.test, ast.Constant):
            continue
        args, expected, compare = assertion(statement.test, task.returns)
        if len(args) != len(task.params):
            raise Untranslatable("candidate called with a different number of arguments")
        try:
            conformed = [
                values.conform(literal(a), t) for a, (_, t) in zip(args, task.params, strict=True)
            ]
            expected = values.conform(expected, task.returns)
        except values.Mismatch as error:
            raise Untranslatable(f"test value: {error}") from None
        found.append(Case(conformed, expected, compare))
    if not found:
        raise Untranslatable("no test translates")
    return found


def docstring(function: ast.FunctionDef) -> str:
    doc = ast.get_docstring(function, clean=False) or ""
    doc = doc.replace("\t", "    ")
    return re.sub(r"\b[Pp]ython (?=function)", "", doc)


def from_multipl_e(task_id: str, source: str) -> Task:
    """The task in one MultiPL-E file, or `Untranslatable`."""
    prompt_part, _, _tests_part = source.partition(UNIT_TESTS)
    module = ast.parse(source)
    boundary = prompt_part.count("\n") + 1
    in_prompt = [s for s in module.body if s.lineno < boundary]
    functions = [s for s in in_prompt if isinstance(s, ast.FunctionDef)]
    others = [
        s for s in in_prompt if not isinstance(s, ast.FunctionDef | ast.Import | ast.ImportFrom)
    ]
    if len(functions) != 1 or others:
        raise Untranslatable("the prompt holds more than one function")
    function = functions[0]
    arguments = function.args
    if (
        arguments.vararg
        or arguments.kwarg
        or arguments.kwonlyargs
        or arguments.posonlyargs
        or arguments.defaults
    ):
        raise Untranslatable("parameters lotml has no form for")
    try:
        params = [(a.arg, types.from_annotation(a.annotation)) for a in arguments.args]
        if function.returns is None or any(a.annotation is None for a in arguments.args):
            raise Untranslatable("an untyped signature")
        returns = types.from_annotation(function.returns)
    except (types.UnsupportedType, AttributeError) as error:
        raise Untranslatable(f"signature: {error}") from None
    if returns == types.Unit():
        raise Untranslatable("a function returning nothing has no result to test")
    body = prompt_part.split(CANONICAL, 1)[1] if CANONICAL in prompt_part else ""
    canonical = (
        "" if body.strip() in ("", "pass") else prompt_part.replace(CANONICAL, "").rstrip() + "\n"
    )
    task = Task(
        id=task_id,
        source=task_id.split("/")[0],
        name=function.name,
        params=params,
        returns=returns,
        doc=docstring(function),
        tests=[],
        canonical=canonical,
    )
    check = next(
        (s for s in module.body if isinstance(s, ast.FunctionDef) and s.name == "check"),
        None,
    )
    if check is None:
        raise Untranslatable("no check function")
    task.tests = cases(check, task)
    return task
