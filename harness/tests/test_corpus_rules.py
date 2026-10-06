"""The mechanical part of translating Python into lotml: what is the same is written back, what
differs by rule is rewritten, and what is not mechanical is left with a reason."""

from lotml_harness.corpus import rules
from lotml_harness.tasks import Task, types

INT = types.Prim("int")


def task(canonical: str, params, returns, name: str = "f") -> Task:
    return Task(
        id="t/1",
        source="t",
        name=name,
        params=params,
        returns=returns,
        doc="",
        tests=[],
        canonical=canonical,
    )


def lotml(canonical: str, params, returns, name: str = "f") -> str:
    translation = rules.translate(task(canonical, params, returns, name))
    assert translation.reasons == [], translation.reasons
    return translation.code


def test_the_signature_comes_from_the_task_and_typing_imports_go():
    code = lotml(
        "from typing import List\n\ndef f(xs: List[int]) -> int:\n    return sum(xs)\n",
        [("xs", types.List(INT))],
        INT,
    )
    assert code == "fn f(xs: [int]) -> int:\n    return sum(xs)\n"


def test_a_local_assigned_again_or_changed_in_place_is_var():
    code = lotml(
        "def f(n):\n    total = 0\n    seen = []\n    for i in range(n):\n        total += i\n"
        "        seen.append(i)\n    best = total\n    return best + len(seen)\n",
        [("n", INT)],
        INT,
    )
    assert "    var total = 0\n" in code
    assert "    var seen = []\n" in code
    assert "    best = total\n" in code, "assigned once, never changed: immutable"


def test_a_parameter_the_body_changes_is_taken_var():
    code = lotml(
        "def f(n, xs):\n    xs.sort()\n    while n > 10:\n        n = n // 10\n"
        "    return n + xs[0]\n",
        [("n", INT), ("xs", types.List(INT))],
        INT,
    )
    assert code.startswith("fn f(var n: int, var xs: [int]) -> int:\n")


def test_truth_is_written_as_the_comparison_lotml_asks_for():
    code = lotml(
        "def f(xs, n, o):\n    if not xs:\n        return 0\n    while n:\n        n -= 1\n"
        "    if o and xs:\n        return 1\n    return 2\n",
        [("xs", types.List(INT)), ("n", INT), ("o", types.Optional(INT))],
        INT,
    )
    assert "    if len(xs) == 0:\n" in code
    assert "    while n != 0:\n" in code
    assert "    if o is not None and len(xs) > 0:\n" in code


def test_a_local_s_truth_follows_its_first_value():
    code = lotml(
        "def f(s):\n    out = ''\n    for c in s:\n        out = out + c\n"
        "    return 1 if out else 0\n",
        [("s", types.Prim("str"))],
        INT,
    )
    assert "return 1 if len(out) > 0 else 0" in code


def test_elif_chains_and_docstrings_keep_their_shape():
    code = lotml(
        'def f(n):\n    """Sign of n."""\n    if n > 0:\n        return 1\n    elif n < 0:\n'
        "        return -1\n    else:\n        return 0\n",
        [("n", INT)],
        INT,
    )
    assert code == (
        'fn f(n: int) -> int:\n    """Sign of n."""\n    if n > 0:\n        return 1\n'
        "    elif n < 0:\n        return -1\n    else:\n        return 0\n"
    )


def test_an_annotated_helper_is_translated_and_math_kept():
    code = lotml(
        "import math\n\ndef half(x: float) -> float:\n    return x / 2\n\n"
        "def f(n):\n    return math.floor(half(n))\n",
        [("n", types.Prim("f64"))],
        INT,
    )
    assert code.startswith("import math\n\nfn half(x: f64) -> f64:\n")


def test_what_is_not_mechanical_is_left_with_a_reason():
    reasons = rules.translate(
        task(
            "import re\n\ndef helper(x):\n    return x\n\ndef f(n):\n    try:\n        return n\n"
            "    except ValueError:\n        raise\n",
            [("n", INT)],
            INT,
        )
    ).reasons
    assert reasons == [
        "a parameter or result with no type",
        "an exception handler",
        "the import `import re`",
    ]


def test_a_nested_function_and_a_loop_else_are_not_mechanical():
    nested = "def f(n):\n    def g(x):\n        return x\n    return g(n)\n"
    assert rules.translate(task(nested, [("n", INT)], INT)).reasons == ["a nested function"]
    loop_else = (
        "def f(n):\n    for i in range(n):\n        pass\n    else:\n        return 1\n"
        "    return 0\n"
    )
    assert rules.translate(task(loop_else, [("n", INT)], INT)).reasons == ["a loop with an `else`"]
