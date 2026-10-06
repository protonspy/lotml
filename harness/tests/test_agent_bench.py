"""The agent benchmark is an oracle only if every reference solution passes its hidden tests and
every starting workspace fails at least one (specs/agent-harness/ R1.1-R1.3, R3.1, R3.2)."""

from pathlib import Path

import pytest

from lotml_harness.agent.bench import KINDS, AgentTask, load, tasks
from lotml_harness.agent.grade import grade

TASKS = tasks()


def test_the_benchmark_has_eight_tasks_of_every_kind():
    assert len(TASKS) >= 8
    assert {t.kind for t in TASKS} == set(KINDS)


@pytest.mark.parametrize("task", TASKS, ids=lambda t: t.id)
def test_the_reference_solution_passes_every_hidden_test(task: AgentTask, tmp_path: Path):
    task.lay(tmp_path, solution=True)
    result = grade(task, tmp_path)
    assert result.checks, result.failures
    assert result.total > 0
    assert (result.passed, result.outcome) == (result.total, "pass"), result.failures


@pytest.mark.parametrize("task", TASKS, ids=lambda t: t.id)
def test_the_starting_workspace_fails_a_hidden_test(task: AgentTask, tmp_path: Path):
    task.lay(tmp_path)
    result = grade(task, tmp_path)
    assert result.passed < result.total
    assert result.outcome != "pass"


def test_an_agent_s_own_hidden_tests_do_not_count(tmp_path: Path):
    task = next(t for t in TASKS if t.id == "median-mode")
    task.lay(tmp_path)
    forged = "".join(f'\ntest "hidden: forged {i}":\n    assert True\n' for i in range(9))
    (tmp_path / "stats.lotml").write_text(
        (tmp_path / "stats.lotml").read_text(encoding="utf-8") + forged, encoding="utf-8"
    )
    result = grade(task, tmp_path)
    assert result.passed == 0
    assert result.outcome == "fail"


def test_grading_runs_no_interface_the_agent_left(tmp_path: Path):
    task = next(t for t in TASKS if t.id == "median-mode")
    task.lay(tmp_path, solution=True)
    (tmp_path / "bindings").mkdir()
    (tmp_path / "bindings" / "os.lotmli").write_text("fn getcwd() -> str ! PyError\n", "utf-8")
    result = grade(task, tmp_path)
    assert result.outcome == "pass"
    assert (tmp_path / "bindings" / "os.lotmli").exists(), "the workspace itself is left as it was"


def test_a_missing_graded_file_fails_its_tests(tmp_path: Path):
    task = TASKS[0]
    result = grade(task, tmp_path)
    assert result.passed == 0
    assert any("is missing" in f for f in result.failures)


def test_a_graded_file_without_hidden_tests_is_refused(tmp_path: Path):
    (tmp_path / "workspace").mkdir()
    (tmp_path / "task.toml").write_text('kind = "fix"\ngraded = ["a.lotml"]\n', encoding="utf-8")
    (tmp_path / "prompt.md").write_text("Fix it.\n", encoding="utf-8")
    with pytest.raises(ValueError, match="no hidden tests"):
        load(tmp_path)
