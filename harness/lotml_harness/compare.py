"""Whether a result passes a hidden test case."""

import json
import math
from typing import Any

from lotml_harness.tasks import Case, types, values

TOLERANCE = 1e-6


def lines(text: str) -> list[str]:
    return [line.rstrip() for line in text.strip().splitlines()]


def matches(actual: Any, case: Case, returns: types.Type) -> bool:
    """`actual` compared with the case's expected value, typed by the return type."""
    if case.compare == "lines":
        return isinstance(actual, str) and lines(actual) == lines(case.expected)
    try:
        result = values.conform(actual, returns)
    except values.Mismatch:
        return False
    if case.compare == "set":
        item = returns.item if isinstance(returns, types.List) else returns

        def key(value: Any) -> str:
            return json.dumps(values.to_json(value, item), sort_keys=True)

        return {key(v) for v in result} == {key(v) for v in case.expected}
    if case.compare == "approx" and isinstance(result, float) and isinstance(case.expected, float):
        return math.isclose(result, case.expected, rel_tol=TOLERANCE, abs_tol=TOLERANCE)
    return result == case.expected
