import pytest

from lotml_harness.compare import matches
from lotml_harness.tasks import Case
from lotml_harness.tasks.types import parse


@pytest.mark.parametrize(
    ("actual", "expected", "compare", "returns", "verdict"),
    [
        (3, 3, "eq", "int", True),
        (3, 4, "eq", "int", False),
        (True, 1, "eq", "int", False),
        ([1, 2], [1, 2], "eq", "[int]", True),
        ((1, 2), [1, 2], "eq", "[int]", True),
        ([1, 2], (1, 2), "eq", "(int, int)", True),
        ("x", [1], "eq", "[int]", False),
        (2, 2.0, "eq", "f64", True),
        (0.1 + 0.2, 0.3, "eq", "f64", False),
        (0.1 + 0.2, 0.3, "approx", "f64", True),
        (0.31, 0.3, "approx", "f64", False),
        ([2, 1, 3], [1, 2, 3], "set", "[int]", True),
        ([[1], [2]], [[2], [1]], "set", "[[int]]", True),
        ([1, 1], [1], "set", "[int]", True),
        ([1], [1, 2], "set", "[int]", False),
        ("a \nb\n\n", "a\nb", "lines", "str", True),
        ("a\nc", "a\nb", "lines", "str", False),
        (None, None, "eq", "int?", True),
        (2**64, 2**64, "eq", "int", False),
    ],
)
def test_matches_compares_a_result_as_its_case_says(
    actual, expected, compare, returns, verdict
):
    case = Case(args=[], expected=expected, compare=compare)
    assert matches(actual, case, parse(returns)) is verdict
