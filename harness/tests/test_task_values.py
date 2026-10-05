import json

import pytest

from lotml_harness.tasks.types import parse
from lotml_harness.tasks.values import (
    I64_MAX,
    Mismatch,
    conform,
    from_json,
    render,
    to_json,
)


@pytest.mark.parametrize(
    ("value", "type_", "expected"),
    [
        (3, "int", 3),
        (3, "f64", 3.0),
        (2.5, "f64", 2.5),
        ("x", "str", "x"),
        (True, "bool", True),
        ([1, 2], "[int]", [1, 2]),
        ((1, 2), "[int]", [1, 2]),
        ((1, "a"), "(int, str)", (1, "a")),
        ([1, "a"], "(int, str)", (1, "a")),
        ({"a": [1]}, "{str: [int]}", {"a": [1]}),
        ({1, 2}, "{int}", {1, 2}),
        ([2, 1], "{int}", {1, 2}),
        (None, "int?", None),
        (4, "int?", 4),
        (None, "None", None),
        ([[1, 2], [3]], "[[f64]]", [[1.0, 2.0], [3.0]]),
    ],
)
def test_conform_checks_and_normalizes_a_value_to_its_type(value, type_, expected):
    result = conform(value, parse(type_))
    assert result == expected
    assert type(result) is type(expected)


@pytest.mark.parametrize(
    ("value", "type_"),
    [
        (True, "int"),
        (1.5, "int"),
        ("1", "int"),
        (None, "int"),
        (I64_MAX + 1, "int"),
        ([1, "a"], "[int]"),
        ((1,), "(int, str)"),
        ({1: 2}, "{str: int}"),
        (1, "None"),
        (float("nan"), "f64"),
        (float("inf"), "f64"),
        ([1, 1], "{int}"),
    ],
)
def test_conform_rejects_values_outside_the_type(value, type_):
    with pytest.raises(Mismatch):
        conform(value, parse(type_))


@pytest.mark.parametrize(
    ("value", "type_", "b", "a"),
    [
        (3, "int", "3", "3"),
        (-3, "int", "-3", "-3"),
        (3.0, "f64", "3.0", "3.0"),
        (1e-07, "f64", "1e-07", "1e-07"),
        ('it\'s "x"\n', "str", '"it\'s \\"x\\"\\n"', '"it\'s \\"x\\"\\n"'),
        (True, "bool", "True", "True"),
        (None, "int?", "None", "none"),
        ([1, 2], "[int]", "[1, 2]", "[1, 2]"),
        ((1, "a"), "(int, str)", '(1, "a")', '(1, "a")'),
        ((1,), "(int,)", "(1,)", "(1,)"),
        ({"k": None}, "{str: int?}", '{"k": None}', '{"k": none}'),
        (set(), "{int}", "set()", "set()"),
        ({2, 1}, "{int}", "{1, 2}", "{1, 2}"),
    ],
)
def test_render_writes_lotml_literals(value, type_, b, a):
    assert render(value, parse(type_), "b") == b
    assert render(value, parse(type_), "a") == a


@pytest.mark.parametrize(
    ("value", "type_"),
    [
        ({3: [(1, "a")]}, "{int: [(int, str)]}"),
        ({1, 2}, "{int}"),
        (None, "f64?"),
        (2.5, "f64"),
        ([(1, 2.0)], "[(int, f64)]"),
    ],
)
def test_json_round_trips_through_the_type(value, type_):
    encoded = json.loads(json.dumps(to_json(value, parse(type_))))
    assert from_json(encoded, parse(type_)) == value
