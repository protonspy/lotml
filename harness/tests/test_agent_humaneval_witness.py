"""A witness signature proves a task's hidden blocks can be met (specs/agent-humaneval/ R1.6)."""

from lotml_harness.agent.humaneval import hidden_blocks, satisfiable, witness
from lotml_harness.tasks.types import List, Optional, Prim, Tuple

INT, F64 = Prim("int"), Prim("f64")


def test_the_witness_is_the_typed_signature_with_todo_and_the_hidden_blocks_after_it():
    blocks = hidden_blocks("add", [((1, 2), 3)], [INT, INT], INT)
    assert witness("add", ["a", "b"], [INT, INT], INT, blocks) == (
        "fn add(a: int, b: int) -> int:\n"
        "    return todo()\n"
        "\n"
        'test "hidden: 1":\n'
        "    assert add(1, 2) == 3\n"
    )


def test_blocks_that_check_against_the_witness_are_satisfiable():
    cases = [(([1.0, 2.5],), (1.0, None)), (([],), (None, 2))]
    params, returns = [List(F64)], Tuple((Optional(F64), Optional(INT)))
    blocks = hidden_blocks("pick", cases, params, returns)
    assert satisfiable(witness("pick", ["xs"], params, returns, blocks)) is None


def test_none_inside_an_expected_container_is_compared_element_by_element():
    params, returns = [List(INT)], Tuple((Optional(INT), Optional(INT)))
    blocks = hidden_blocks("ends", [(([1],), (None, 1)), (([],), (None, None))], params, returns)
    assert "    r0, r1 = ends([1])\n    assert r0 is None\n    assert r1 == 1\n" in blocks
    assert satisfiable(witness("ends", ["xs"], params, returns, blocks)) is None
    listed = hidden_blocks("holes", [((2,), [None, 1])], [INT], List(Optional(INT)))
    assert "    assert r0[0] is None\n    assert r0[1] == 1\n" in listed
    assert satisfiable(witness("holes", ["n"], [INT], List(Optional(INT)), listed)) is None


def test_blocks_that_do_not_check_are_refused_with_the_compiler_s_first_error():
    blocks = 'test "hidden: 1":\n    assert add(1, 2) == "three"\n'
    reason = satisfiable(witness("add", ["a", "b"], [INT, INT], INT, blocks))
    assert reason is not None
    assert reason.startswith("error")
