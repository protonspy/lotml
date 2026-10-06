"""Recording a problem's cases from its canonical solution (specs/agent-humaneval/ R1.2, R3.3)."""

from lotml_harness.agent import humaneval
from lotml_harness.agent.humaneval import NO_LITERAL, record

DOUBLE = "def double(xs):\n    return [2 * x for x in xs]\n"


def test_every_call_check_makes_is_a_case_in_the_order_made_with_loops_and_random_inputs():
    test = (
        "import random\n"
        "def check(candidate):\n"
        "    assert candidate([1, 2]) == [2, 4]\n"
        "    for n in range(3):\n"
        "        assert candidate([n]) == [2 * n]\n"
        "    x = random.randint(0, 10**9)\n"
        "    assert candidate([x]) == [2 * x]\n"
    )
    first = record(DOUBLE, test, "double", "check")
    assert first.error is None
    assert first.cases[:4] == [(([1, 2],), [2, 4]), (([0],), [0]), (([1],), [2]), (([2],), [4])]
    assert len(first.cases) == 5
    assert record(DOUBLE, test, "double", "check").cases == first.cases, "random is seeded"


def test_duplicates_are_dropped_and_the_first_fifty_kept():
    test = (
        "def check(candidate):\n"
        "    for n in range(80):\n"
        "        candidate([n % 60])\n"
        "        candidate([n % 60])\n"
    )
    recorded = record(DOUBLE, test, "double", "check")
    assert len(recorded.cases) == humaneval.CASES == 50
    assert [args[0][0] for args, _ in recorded.cases] == list(range(50))


def test_arguments_are_copied_before_the_call_so_a_solution_that_changes_them_is_recorded_right():
    code = "def pop_all(xs):\n    n = len(xs)\n    xs.clear()\n    return n\n"
    test = "def check(candidate):\n    assert candidate([7, 8]) == 2\n"
    assert record(code, test, "pop_all", "check").cases == [(([7, 8],), 2)]


def test_the_child_sees_no_key_of_the_parent_and_an_empty_working_directory(monkeypatch):
    monkeypatch.setenv("OPENROUTER_API_KEY", "sk-secret")
    code = (
        "import os\n"
        "def look():\n"
        "    return (os.environ.get('OPENROUTER_API_KEY'), os.listdir('.'))\n"
    )
    test = "def check(candidate):\n    candidate()\n"
    assert record(code, test, "look", "check").cases == [((), (None, []))]


def test_asserts_calling_the_function_by_name_record_only_the_outermost_calls():
    code = "def fact(n):\n    return 1 if n < 2 else n * fact(n - 1)\n"
    test = "assert fact(4) == 24\nassert fact(3) == 6\n"
    assert record(code, test, "fact", "asserts").cases == [((4,), 24), ((3,), 6)]


def test_setup_code_runs_after_the_code_and_before_the_asserts():
    code = "def scale(x):\n    return x * K\n"
    assert record(code, "assert scale(2) == 6", "scale", "asserts", setup="K = 3").cases == [
        ((2,), 6)
    ]


def test_a_value_with_no_python_literal_is_marked_rather_than_guessed():
    code = "class Box:\n    pass\ndef box():\n    return Box()\n"
    test = "def check(candidate):\n    candidate()\n"
    assert record(code, test, "box", "check").cases == [((), NO_LITERAL)]


def test_a_failing_test_a_hang_and_an_oversized_output_are_errors(monkeypatch):
    failing = "def check(candidate):\n    assert candidate([1]) == [3]\n"
    assert record(DOUBLE, failing, "double", "check").error == "test: AssertionError"
    hang = "def check(candidate):\n    while True:\n        pass\n"
    assert record(DOUBLE, hang, "double", "check", seconds=2).error == "timeout"
    monkeypatch.setattr(humaneval, "RECORD_OUTPUT", 100)
    large = "def check(candidate):\n    candidate(list(range(1000)))\n"
    assert record(DOUBLE, large, "double", "check").error == "output"


def test_keyword_arguments_are_refused():
    test = "def check(candidate):\n    candidate(xs=[1])\n"
    assert record(DOUBLE, test, "double", "check").error == "test: TypeError"
