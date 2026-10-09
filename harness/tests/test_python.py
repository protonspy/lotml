"""The CPython the harness's runs use (plans/python-via-uv.md 3.1): provisioned ahead of a run,
then every run offline, a row with no interpreter failing by name instead of downloading."""

import json
import subprocess
from pathlib import Path

from lotml_harness import python
from lotml_harness.execute import child_environment


def test_a_child_runs_offline_on_the_interpreter_lotml_resolves_not_the_harness_s_own(monkeypatch):
    monkeypatch.delenv("LOTML_PYTHON", raising=False)
    env = child_environment()
    assert env["LOTML_OFFLINE"] == "1"
    assert "LOTML_PYTHON" not in env, "lotml resolves CPython 3.14 itself"
    monkeypatch.setenv("LOTML_PYTHON", "/given/python")
    assert child_environment()["LOTML_PYTHON"] == "/given/python", "a run can still be held to one"


def test_the_provisioned_interpreter_is_cpython_3_14_and_recorded():
    found = python.interpreter()
    assert found is not None, "run `python -m lotml_harness.python --provision` first"
    assert str(found["version"]).startswith("3.14"), found
    assert Path(found["path"]).is_absolute(), found


def test_a_row_with_no_interpreter_fails_naming_what_is_missing_and_downloads_nothing(
    tmp_path, monkeypatch
):
    monkeypatch.delenv("LOTML_PYTHON", raising=False)
    empty = tmp_path / "pythons"
    empty.mkdir()
    probe = tmp_path / "probe.lotml"
    probe.write_text(python.PROBE, encoding="utf-8", newline="\n")
    env = child_environment() | {"UV_PYTHON_INSTALL_DIR": str(empty)}
    ran = subprocess.run(  # noqa: S603 - the compiler, with fixed arguments
        [str(python.COMPILER), "run", "--json", str(probe)],
        capture_output=True,
        text=True,
        encoding="utf-8",
        env=env,
        cwd=tmp_path,
        timeout=120,
        check=False,
    )
    assert ran.returncode != 0
    said = ran.stdout + ran.stderr
    assert "CPython 3.14 is not installed through uv" in said, said
    assert "downloads nothing" in said, said
    assert not any(empty.iterdir()), "nothing was downloaded"
    reports = [json.loads(line) for line in ran.stdout.splitlines() if line.startswith("{")]
    assert all(r.get("python") is None for r in reports), reports
