"""The forgotten-`await` hypothesis: concurrency tasks written with Python's asyncio and with
lotml's colorless `parallel`, and the concurrency mistakes each answer makes."""

import pytest

from lotml_harness.experiments import awaits
from lotml_harness.experiments.phase1 import Lotml


def test_the_task_set_has_both_languages_and_tests_from_a_canonical_solution():
    tasks = awaits.tasks()
    assert len(tasks) >= 30
    assert len({t.id for t in tasks}) == len(tasks)
    task = tasks[0]
    assert "async def" in task.python_helpers and "fn " in task.lotml_helpers
    assert task.task.tests, "the hidden tests come from the canonical solution"


@pytest.mark.parametrize("task", awaits.tasks()[::7], ids=lambda t: t.id)
def test_each_canonical_solution_passes_in_both_languages(task):
    python = awaits.judge_python(task, task.python_canonical)
    assert python.passed and python.concurrent and python.mistake is None, python
    lotml = awaits.judge_lotml(task, task.lotml_canonical, Lotml())
    assert lotml.passed and lotml.concurrent and lotml.mistake is None, lotml


def first_fetch_task():
    return next(t for t in awaits.tasks() if t.template == "fetch_all")


def test_a_coroutine_never_awaited_is_a_forgotten_await():
    task = first_fetch_task()
    helper = task.helper_names[0]
    forgot = f"async def {task.name}(keys: list[str]) -> list[int]:\n    return [{helper}(k) for k in keys]\n"
    judged = awaits.judge_python(task, forgot)
    assert not judged.passed and judged.mistake == "forgotten await"


def test_awaiting_one_by_one_is_right_but_not_concurrent():
    task = first_fetch_task()
    helper = task.helper_names[0]
    sequential = (
        f"async def {task.name}(keys: list[str]) -> list[int]:\n"
        f"    return [await {helper}(k) for k in keys]\n"
    )
    judged = awaits.judge_python(task, sequential)
    assert judged.passed and not judged.concurrent and judged.mistake is None


def test_running_a_loop_inside_the_loop_is_a_colour_mistake():
    task = first_fetch_task()
    helper = task.helper_names[0]
    nested = (
        f"import asyncio\n\nasync def {task.name}(keys: list[str]) -> list[int]:\n"
        f"    return [asyncio.run({helper}(k)) for k in keys]\n"
    )
    assert awaits.judge_python(task, nested).mistake == "colour"


def test_lotml_answers_are_judged_by_the_compiler_and_the_tests():
    task = first_fetch_task()
    helper = task.helper_names[0]
    lotml = Lotml()
    leaked = f"fn {task.name}(keys: [str]) -> [int]:\n    return [await {helper}(k) for k in keys]\n"
    assert awaits.judge_lotml(task, leaked, lotml).mistake == "async or await"
    misused = f"fn {task.name}(keys: [str]) -> [int]:\n    return parallel({helper}(keys[0]))\n"
    assert awaits.judge_lotml(task, misused, lotml).mistake == "tasks misused"
    sequential = f"fn {task.name}(keys: [str]) -> [int]:\n    return [{helper}(k) for k in keys]\n"
    judged = awaits.judge_lotml(task, sequential, lotml)
    assert judged.passed and not judged.concurrent


def test_the_summary_counts_mistakes_and_pairs_the_languages():
    rows = [
        {"model": "m", "task": "a", "language": "python", "passed": False, "mistake": "forgotten await", "concurrent": True},
        {"model": "m", "task": "a", "language": "lotml", "passed": True, "mistake": None, "concurrent": True},
        {"model": "m", "task": "b", "language": "python", "passed": True, "mistake": None, "concurrent": False},
        {"model": "m", "task": "b", "language": "lotml", "passed": True, "mistake": None, "concurrent": True},
    ]  # fmt: skip
    s = awaits.summarize(rows)["m"]
    assert s["pairs"] == 2
    assert s["mistakes"]["python"] == {"forgotten await": 1}
    assert s["mistakes"]["lotml"] == {}
    assert (s["only_python_mistake"], s["only_lotml_mistake"]) == (1, 0)
    assert s["passed"] == {"python": 1, "lotml": 2}
    assert s["concurrent"] == {"python": 1, "lotml": 2}
