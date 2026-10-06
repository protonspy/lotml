"""The safe layer: scratch copies of untrusted files, and confined `lotml` calls
(specs/seeded-failures/ R2.7, specs/guide-evaluation/ R2.8)."""

import sys
import time
from pathlib import Path

import pytest

from lotml_harness.agent import safe


@pytest.mark.parametrize(
    "name",
    [
        "/a.lotml",
        "C:/a.lotml",
        "..\\a.lotml",
        "a/../../b.lotml",
        "a.txt",
        "a.lotmli",
        "CON.lotml",
        "nul.lotml",
        "sub/COM1.lotml",
        "a.lotml.",
        "",
    ],
)
def test_a_name_that_is_absolute_climbs_names_a_device_or_is_not_lotml_is_refused(name):
    with pytest.raises(ValueError):
        safe.checked_name(name)


def test_lay_writes_lotml_files_and_nothing_when_one_name_is_refused(tmp_path: Path):
    assert safe.lay(tmp_path / "ok", {"a.lotml": "x", "src/b.lotml": "y"}) == [
        "a.lotml",
        "src/b.lotml",
    ]
    assert (tmp_path / "ok" / "src" / "b.lotml").read_text(encoding="utf-8") == "y"
    safe.lay(tmp_path / "lines", {"a.lotml": "x\ny\n"})
    assert (tmp_path / "lines" / "a.lotml").read_bytes() == b"x\ny\n", "written byte for byte"
    with pytest.raises(ValueError):
        safe.lay(tmp_path / "bad", {"a.lotml": "x", "b.lotmli": "y"})
    assert not (tmp_path / "bad").exists()


PYTHON = sys.executable


def test_files_come_after_a_double_dash_in_a_clean_environment(tmp_path: Path, monkeypatch):
    monkeypatch.setenv("OPENROUTER_API_KEY", "sk-secret-value")
    show = "import os, sys; print(sys.argv[1:], os.environ.get('OPENROUTER_API_KEY'))"
    done = safe.lotml(["-c", show], ["-a.lotml"], tmp_path, binary=PYTHON)
    assert done is not None
    assert done.returncode == 0
    assert done.stdout.strip() == "['--', '-a.lotml'] None"


def test_the_deadline_ends_the_call_and_every_process_it_started(tmp_path: Path):
    marker = tmp_path / "late.txt"
    child = f"import time, pathlib; time.sleep(3); pathlib.Path({str(marker)!r}).write_text('x')"
    parent = (
        "import subprocess, sys, time; "
        f"subprocess.Popen([sys.executable, '-c', {child!r}]); time.sleep(60)"
    )
    started = time.monotonic()
    assert safe.lotml(["-c", parent], [], tmp_path, deadline=1.5, binary=PYTHON) is None
    assert time.monotonic() - started < 10
    time.sleep(4)
    assert not marker.exists(), "a process the call started outlived its deadline"


def test_the_memory_cap_holds(tmp_path: Path):
    grab = "x = bytearray(800 * 2**20); print('allocated')"
    done = safe.lotml(["-c", grab], [], tmp_path, binary=PYTHON, memory=200 * 2**20)
    assert done is not None
    assert done.returncode != 0
    assert "allocated" not in done.stdout
