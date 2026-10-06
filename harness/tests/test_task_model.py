from lotml_harness.tasks import Case, Task
from lotml_harness.tasks.types import List, Optional, Prim


def task() -> Task:
    return Task(
        id="humaneval/1",
        source="humaneval",
        name="first",
        params=[("xs", List(Prim("int")))],
        returns=Optional(Prim("int")),
        doc=" The first element, or None.\n    >>> first([])\n    None\n    ",
        tests=[Case(args=[[1, 2]], expected=1, compare="eq")],
    )


def test_prompt_is_the_signature_and_docstring_in_each_variant():
    assert task().prompt("b") == (
        "fn first(xs: [int]) -> int?:\n"
        '    """ The first element, or None.\n'
        "    >>> first([])\n"
        "    None\n"
        '    """\n'
    )
    assert "or none.\n    >>> first([])\n    none\n" in task().prompt("a")


def test_a_function_returning_the_unit_type_omits_the_arrow():
    from lotml_harness.tasks.types import Unit

    unit = Task(**{**task().__dict__, "returns": Unit(), "doc": ""})
    assert unit.prompt("b") == "fn first(xs: [int]):\n"


def test_task_round_trips_through_json():
    original = task()
    assert Task.from_json(original.to_json()) == original


def test_case_json_is_typed_by_the_signature():
    record = task().to_json()
    assert record["params"] == [["xs", "[int]"]]
    assert record["returns"] == "int?"
    assert record["tests"] == [{"args": [[1, 2]], "expected": 1, "compare": "eq"}]
