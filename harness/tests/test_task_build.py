from lotml_harness.tasks import Case, Task
from lotml_harness.tasks.build import Report, translate, validate
from lotml_harness.tasks.types import List, Prim

GOOD = '''from typing import List

def total(xs: List[int]) -> int:
    """ Sum of xs. """
    ### Canonical solution below ###
    return sum(xs)

### Unit tests below ###
def check(candidate):
    assert candidate([1, 2]) == 3
'''


def task(canonical: str, expected: int = 3) -> Task:
    return Task(
        id="mbpp/1",
        source="mbpp",
        name="total",
        params=[("xs", List(Prim("int")))],
        returns=Prim("int"),
        doc="",
        tests=[Case([[1, 2]], expected)],
        canonical=canonical,
    )


def test_translate_keeps_tasks_and_counts_refusals_by_reason():
    report = Report()
    tasks = translate(
        {"humaneval/1": GOOD, "humaneval/2": GOOD.replace("List[int]", "Any")},
        report,
    )
    assert [t.id for t in tasks] == ["humaneval/1"]
    assert report.refused == {"humaneval": {"signature": 1}}


def test_validate_keeps_tasks_whose_canonical_solution_passes():
    report = Report()
    source = "def total(xs):\n    return sum(xs)\n"
    kept = validate([task(source), task(source, expected=4)], report)
    assert [t.tests[0].expected for t in kept] == [3]
    assert report.refused == {"mbpp": {"canonical fails": 1}}


def test_validate_survives_a_canonical_solution_that_hangs_or_crashes():
    report = Report()
    hang = "def total(xs):\n    while True:\n        pass\n"
    crash = "def total(xs):\n    raise SystemExit(3)\n"
    assert validate([task(hang), task(crash)], report, timeout=2) == []
    assert report.refused == {"mbpp": {"canonical fails": 2}}


def test_validate_keeps_tasks_with_no_canonical_solution_unchecked():
    report = Report()
    kept = validate([task("")], report)
    assert len(kept) == 1
    assert report.unchecked == {"mbpp": 1}
