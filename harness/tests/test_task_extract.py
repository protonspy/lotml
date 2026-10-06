import pytest

from lotml_harness.tasks.extract import Untranslatable, from_multipl_e

HUMANEVAL = '''from typing import List


def below(numbers: List[float], threshold: float) -> bool:
    """ Is every number below the threshold?
    >>> below([1.0, 2.0], 3.0)
    True
    """
    ### Canonical solution below ###
    return all(n < threshold for n in numbers)

### Unit tests below ###


METADATA = {}


def check(candidate):
    assert candidate([1.0, 2.0], 3.0) == True
    assert False == candidate([4.0], 3)
    assert abs(candidate([1.0], 2.0) - True) < 1e-6
    assert True

def test_check():
    check(below)
'''

MBPP = '''from typing import List

def evens(lst: List[int]) -> List[int]:
    """
\tWrite a python function to return the even numbers of the given list.
\t"""
    ### Canonical solution below ###
    pass

### Unit tests below ###
def check(candidate):
    assert candidate([1, 2, 4]) == [2, 4]
    assert not candidate([1])
    assert set(candidate([4, 2])) == set([2, 4])

def test_check():
    check(evens)
'''


def test_humaneval_file_becomes_a_task():
    task = from_multipl_e("humaneval/7", HUMANEVAL)
    assert task.name == "below"
    assert task.source == "humaneval"
    assert [(n, t.render("b")) for n, t in task.params] == [
        ("numbers", "[f64]"),
        ("threshold", "f64"),
    ]
    assert task.returns.render("b") == "bool"
    assert task.doc.startswith(" Is every number below")
    assert "return all(n < threshold" in task.canonical
    assert [(c.args, c.expected, c.compare) for c in task.tests] == [
        ([[1.0, 2.0], 3.0], True, "eq"),
        ([[4.0], 3.0], False, "eq"),
        ([[1.0], 2.0], True, "approx"),
    ]


def test_mbpp_file_drops_the_python_wording_and_keeps_its_comparisons():
    task = from_multipl_e("mbpp/105", MBPP)
    assert "Write a function to return the even numbers" in task.doc
    assert task.canonical == ""
    assert [(c.expected, c.compare) for c in task.tests] == [
        ([2, 4], "eq"),
        ([], "eq"),
        ([2, 4], "set"),
    ]


@pytest.mark.parametrize(
    ("old", "new"),
    [
        (
            "    assert candidate([1.0, 2.0], 3.0) == True\n",
            "    for x in []:\n        assert candidate(x, 1.0)\n",
        ),
        (
            "candidate([1.0, 2.0], 3.0) == True",
            "candidate(list(range(3)), 3.0) == True",
        ),
        ("def below(numbers: List[float]", "def below(numbers: Any"),
        ("-> bool:", "-> None:"),
        ("candidate([1.0, 2.0], 3.0) == True", "candidate([1.0, 2.0], 3.0) == 2**70"),
        ("candidate([1.0, 2.0], 3.0) == True", "candidate([1.0, 2.0], 3.0) == 7"),
        ("    assert True\n", "    assert below([1.0], 2.0)\n"),
    ],
)
def test_tasks_lotml_cannot_test_faithfully_are_refused(old, new):
    assert old in HUMANEVAL
    with pytest.raises(Untranslatable):
        from_multipl_e("humaneval/7", HUMANEVAL.replace(old, new))


def test_helper_functions_in_the_prompt_are_refused():
    source = HUMANEVAL.replace(
        "def below", "def helper(x: int) -> int:\n    return x\n\n\ndef below"
    )
    with pytest.raises(Untranslatable):
        from_multipl_e("humaneval/7", source)


def test_a_task_without_tests_is_refused():
    source = MBPP.split("def check")[0] + "def check(candidate):\n    pass\n"
    with pytest.raises(Untranslatable):
        from_multipl_e("mbpp/105", source)
