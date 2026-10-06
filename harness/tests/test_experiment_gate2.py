"""The phase 2 gate: Python calling lotml through the checked boundary, lotml calling Python
through a generated interface, and the corpus validated by its own tests."""

from lotml_harness.experiments import gate2
from lotml_harness.tasks import Case, Task, types

TASK = Task(
    id="t/1",
    source="t",
    name="double",
    params=[("n", types.Prim("int"))],
    returns=types.Prim("int"),
    doc="",
    tests=[Case([2], 4), Case([5], 10)],
)
RIGHT = 'fn double(n: int) -> int:\n    return n * 2\n\ntest "double":\n    assert double(2) == 4\n'
WRONG = 'fn double(n: int) -> int:\n    return n + 2\n\ntest "double":\n    assert double(2) == 5\n'


def entry(code: str) -> dict:
    return {"task": TASK.id, "lotml": code, "tests": 1}


def test_python_calls_each_program_through_the_boundary_on_its_tests():
    check = gate2.python_calls_lotml([entry(RIGHT), entry(WRONG)], {TASK.id: TASK})
    assert (check.total, check.passed) == (2, 1)
    assert check.detail == ["t/1: 1 failed; "], "double(5) is 7, not 10"


def test_lotml_calls_python_s_textwrap_through_the_interface_it_bound():
    assert gate2.lotml_calls_python().passed == 1


def test_a_program_whose_own_test_fails_is_named():
    check = gate2.corpus_tests([entry(RIGHT), entry(WRONG)])
    assert (check.total, check.passed, check.detail) == (2, 1, ["t/1"])


def test_the_gate_passes_only_when_every_criterion_does():
    ok, bad = gate2.Check(2, 2, []), gate2.Check(2, 1, ["t/1"])
    assert gate2.markdown(ok, ok, ok).rstrip().endswith("The gate passes.")
    assert gate2.markdown(ok, bad, ok).rstrip().endswith("The gate does not pass.")
