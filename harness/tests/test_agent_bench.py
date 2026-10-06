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


def a_task(tmp_path: Path) -> Path:
    directory = tmp_path / "task"
    for path, text in {
        "task.toml": 'kind = "fix"\ngraded = ["src/a.lotml"]\n',
        "prompt.md": "Fix it.\n",
        "workspace/src/a.lotml": "fn a() -> int:\n    return 0\n",
        "workspace/lotml.toml": "",
        "hidden/src/a.lotml": 'test "hidden: a":\n    assert a() == 1\n',
        "solution/src/a.lotml": "fn a() -> int:\n    return 1\n",
    }.items():
        (directory / path).parent.mkdir(parents=True, exist_ok=True)
        (directory / path).write_text(text, encoding="utf-8", newline="")
    return directory


def test_a_task_carries_its_files_as_data_read_from_its_directory(tmp_path: Path):
    task = load(a_task(tmp_path))
    assert task.workspace_files == {
        "lotml.toml": "",
        "src/a.lotml": "fn a() -> int:\n    return 0\n",
    }
    assert task.hidden_files == {"src/a.lotml": 'test "hidden: a":\n    assert a() == 1\n'}
    assert task.solution_files == {"src/a.lotml": "fn a() -> int:\n    return 1\n"}
    assert task.hidden("src/a.lotml") == task.hidden_files["src/a.lotml"]


def test_lay_writes_the_workspace_then_with_solution_the_reference_over_it(tmp_path: Path):
    task = load(a_task(tmp_path))
    started, solved = tmp_path / "started", tmp_path / "solved"
    task.lay(started)
    task.lay(solved, solution=True)
    assert (started / "src" / "a.lotml").read_text(encoding="utf-8").endswith("return 0\n")
    assert (solved / "src" / "a.lotml").read_text(encoding="utf-8").endswith("return 1\n")
    assert (solved / "lotml.toml").exists()


def test_a_task_needs_no_directory():
    task = AgentTask(
        id="t",
        kind="implement",
        graded=("a.lotml",),
        prompt="Write a.",
        workspace_files={"a.lotml": "fn a() -> int:\n    return todo()\n"},
        hidden_files={"a.lotml": 'test "hidden: 1":\n    assert a() == 1\n'},
    )
    assert task.directory is None
    assert task.solution_files == {}
    assert task.hidden("a.lotml").startswith('test "hidden: 1"')


@pytest.mark.parametrize("name", ["../out.lotml", "a/../../out.lotml", "/abs.lotml", "C:/x.lotml"])
def test_lay_refuses_a_name_that_is_absolute_holds_dot_dot_or_leaves_the_target(name, tmp_path):
    task = AgentTask(
        id="t",
        kind="implement",
        graded=("a.lotml",),
        prompt="",
        workspace_files={name: "x"},
        hidden_files={},
    )
    target = tmp_path / "target"
    with pytest.raises(ValueError, match="outside"):
        task.lay(target)
    assert not (tmp_path / "out.lotml").exists()
