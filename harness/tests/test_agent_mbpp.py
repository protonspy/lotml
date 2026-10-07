"""MBPP's original release as agent tasks (specs/agent-humaneval/ R3.1-R3.6)."""

import pytest

from lotml_harness.agent import humaneval
from lotml_harness.agent.humaneval import Refused, called_function, pose_mbpp

PROBLEM = {
    "task_id": 12,
    "text": "Write a function to add two numbers.",
    "code": "def add(a, b):\r\n\treturn a + b",
    "test_setup_code": "",
    "test_list": ["assert add(1, 2) == 3", "assert add(-1, 1) == 0", "assert add(0, 0) == 0"],
}


def test_the_called_function_is_the_one_of_its_code_every_assert_calls():
    assert called_function(PROBLEM) == "add"
    code = "def helper(x):\n    return x\ndef add(a, b):\n    return helper(a) + b"
    helper = PROBLEM | {"code": code}
    assert called_function(helper) == "add", "a helper the asserts do not call is not tested"


@pytest.mark.parametrize(
    "tests",
    [
        ["assert len([1]) == 1"],
        ["assert add(1, 2) == 3", "assert sub(1, 2) == -1"],
    ],
    ids=["calls none", "calls two"],
)
def test_a_problem_whose_tests_call_none_or_several_of_its_functions_is_refused(tests):
    code = "def add(a, b):\n    return a + b\ndef sub(a, b):\n    return a - b"
    with pytest.raises(Refused) as refused:
        called_function(PROBLEM | {"code": code, "test_list": tests})
    assert refused.value.reason == "tests"


def test_an_mbpp_problem_is_posed_as_humaneval_s_are():
    task = pose_mbpp(PROBLEM)
    assert task.id == "mbpp-12"
    assert task.workspace_files == {
        "solution.lot": (
            'fn add(a, b):\n    """Write a function to add two numbers."""\n    return todo()\n'
        )
    }
    assert task.prompt == humaneval.PROMPT.format(name="add")
    assert task.hidden("solution.lot").count('test "hidden:') == 3


def test_setup_code_runs_before_the_asserts_and_recursion_records_the_outermost_call():
    recursive = PROBLEM | {
        "code": "def fact(n):\n    return 1 if n < 2 else n * fact(n - 1)",
        "test_list": ["assert fact(K) == 24"],
        "test_setup_code": "K = 4",
    }
    recorded = humaneval.record_mbpp(recursive)
    assert recorded.cases == [((4,), 24)]


def test_mbpp_pins_a_full_commit_and_a_sha_256():
    from lotml_harness.tasks import sources

    assert len(sources.MBPP) == 40
    assert len(sources.MBPP_SHA256) == 64
