"""Recorded values as lotml literals and hidden blocks (specs/agent-humaneval/ R1.5, R1.6)."""

import pytest

from lotml_harness.agent.humaneval import NO_LITERAL, NoLiteral, column, hidden_blocks, literal
from lotml_harness.tasks.types import Dict, List, Optional, Prim, Set, Tuple

INT, F64, STR, BOOL = Prim("int"), Prim("f64"), Prim("str"), Prim("bool")


@pytest.mark.parametrize(
    ("values", "expected"),
    [
        ([True, False], BOOL),
        ([7, -1], INT),
        ([7.5], F64),
        (["a"], STR),
        ([[1, 2], []], List(INT)),
        ([(1, "a")], Tuple((INT, STR))),
        ([{1, 2}, set()], Set(INT)),
        ([{"a": [1]}, {}], Dict(STR, List(INT))),
    ],
)
def test_a_column_is_typed_by_its_values_python_types(values, expected):
    assert column(values) == expected


def test_numbers_mixing_int_and_float_are_f64_and_their_integers_are_written_as_floats():
    assert column([[1, 2.5]]) == List(F64)
    assert literal([1, 2.5], List(F64)) == "[1.0, 2.5]"
    assert column([1, 2.5]) == F64
    assert literal(1, F64) == "1.0"


def test_none_makes_a_column_optional():
    assert column([None, 3]) == Optional(INT)
    assert column([(None, 1), (2, None), (None, None)]) == Tuple((Optional(INT), Optional(INT)))
    assert literal(None, Optional(INT)) == "None"
    assert literal([None, 3], List(Optional(INT))) == "[None, 3]"


def test_an_empty_container_alone_takes_int_for_its_items():
    assert column([[]]) == List(INT)
    assert column([{}]) == Dict(INT, INT)


def test_literals_are_written_as_lotml_writes_them():
    assert literal((1, "a"), Tuple((INT, STR))) == '(1, "a")'
    assert literal(set(), Set(INT)) == "set()"
    assert literal({2, 1}, Set(INT)) == "{1, 2}"
    assert literal({"a": [1]}, Dict(STR, List(INT))) == '{"a": [1]}'
    assert literal('say "hi"\n', STR) == '"say \\"hi\\"\\n"'
    assert literal(True, BOOL) == "True"


@pytest.mark.parametrize(
    "values",
    [
        [[1, "a"]],
        [1, "a"],
        [[True, 1]],
        [NO_LITERAL],
        [float("inf")],
        [float("nan")],
        [2**70],
        [(1, 2), (1, 2, 3)],
        [()],
        [None],
        [[{}, 1]],
    ],
    ids=[
        "mixed list",
        "mixed column",
        "bool and int",
        "object",
        "inf",
        "nan",
        "overflow",
        "tuple lengths",
        "empty tuple",
        "only None",
        "empty dict and int",
    ],
)
def test_a_value_with_no_lotml_form_is_refused(values):
    with pytest.raises(NoLiteral):
        column(values)


def test_exact_results_are_compared_with_equality():
    cases = [((1, [2, 3]), 5), ((0, []), 0)]
    assert hidden_blocks("add", cases, [INT, List(INT)], INT) == (
        'test "hidden: 1":\n'
        "    assert add(1, [2, 3]) == 5\n"
        "\n"
        'test "hidden: 2":\n'
        "    assert add(0, []) == 0\n"
    )


def test_a_none_result_is_compared_with_is_none():
    blocks = hidden_blocks("find", [(("a",), None), (("b",), 2)], [STR], Optional(INT))
    assert '    assert find("a") is None\n' in blocks
    assert '    assert find("b") == 2\n' in blocks


def test_a_float_result_compares_within_the_tolerance():
    blocks = hidden_blocks("mean", [(([1, 2.5],), 1.75)], [List(F64)], F64)
    assert blocks == (
        'test "hidden: 1":\n'
        "    assert abs(mean([1.0, 2.5]) - 1.75) <= 1e-06 * max(1.0, abs(1.75))\n"
    )


def test_floats_inside_lists_tuples_and_optionals_compare_element_by_element():
    blocks = hidden_blocks("scale", [(([1.0],), [0.5, 1.0])], [List(F64)], List(F64))
    assert blocks == (
        'test "hidden: 1":\n'
        "    r0 = scale([1.0])\n"
        "    assert len(r0) == 2\n"
        "    assert abs(r0[0] - 0.5) <= 1e-06 * max(1.0, abs(0.5))\n"
        "    assert abs(r0[1] - 1.0) <= 1e-06 * max(1.0, abs(1.0))\n"
    )
    pair = hidden_blocks("closest", [(([1.0, 2.0],), (1.0, 2.0))], [List(F64)], Tuple((F64, F64)))
    assert "    r0, r1 = closest([1.0, 2.0])\n" in pair
    assert "    assert abs(r1 - 2.0) <= 1e-06 * max(1.0, abs(2.0))\n" in pair
    maybe = hidden_blocks("root", [((4.0,), 2.0)], [F64], Optional(F64))
    assert maybe == (
        'test "hidden: 1":\n'
        "    r0 = root(4.0)\n"
        "    assert r0 is not None\n"
        "    if r0 is not None:\n"
        "        assert abs(r0 - 2.0) <= 1e-06 * max(1.0, abs(2.0))\n"
    )
