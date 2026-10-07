"""The Windows registration script, run for real against a throwaway key under HKEY_CURRENT_USER
(specs/file-icons/ R2.1-R2.3)."""

import os
import shutil
import subprocess
import sys
from pathlib import Path

import pytest

from lotml_harness import ROOT

if sys.platform != "win32":
    pytest.skip("the registry is Windows'", allow_module_level=True)

import winreg

SCRIPT = ROOT / "editors" / "windows" / "register.ps1"
POWERSHELL = shutil.which("powershell")
TEST_KEY = rf"Software\LotML-test-{os.getpid()}"
CLASSES = rf"{TEST_KEY}\Classes"
MACHINE = rf"{TEST_KEY}\Machine"
"""Stands for HKLM's classes, which the script reads and never writes."""


def run(icon_home: Path, *flags: str) -> subprocess.CompletedProcess:
    assert POWERSHELL, "Windows PowerShell"
    args = [POWERSHELL, "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", str(SCRIPT)]
    args += ["-Root", rf"HKCU:\{CLASSES}", "-MachineRoot", rf"HKCU:\{MACHINE}"]
    args += ["-IconHome", str(icon_home), *flags]
    done = subprocess.run(args, capture_output=True, text=True, check=False)  # noqa: S603
    assert done.returncode == 0, done.stderr
    return done


def default(path: str) -> str | None:
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, path) as key:
            return winreg.QueryValueEx(key, "")[0]
    except FileNotFoundError:
        return None


def exists(path: str) -> bool:
    try:
        winreg.OpenKey(winreg.HKEY_CURRENT_USER, path).Close()
    except FileNotFoundError:
        return False
    return True


def set_value(path: str, name: str, value: str) -> None:
    with winreg.CreateKey(winreg.HKEY_CURRENT_USER, path) as key:
        winreg.SetValueEx(key, name, 0, winreg.REG_SZ, value)


def delete_tree(path: str) -> None:
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, path) as key:
            while True:
                try:
                    child = winreg.EnumKey(key, 0)
                except OSError:
                    break
                delete_tree(rf"{path}\{child}")
        winreg.DeleteKey(winreg.HKEY_CURRENT_USER, path)
    except FileNotFoundError:
        pass


@pytest.fixture
def classes():
    delete_tree(TEST_KEY)
    yield CLASSES
    delete_tree(TEST_KEY)


def test_both_extensions_get_the_lotml_type_whose_icon_is_a_copy(classes, tmp_path: Path):
    home = tmp_path / "LotML"
    said = run(home).stdout
    icon = home / "lotml-file.ico"
    assert icon.read_bytes() == (ROOT / "editors" / "icons" / "lotml-file.ico").read_bytes()
    assert default(rf"{classes}\.lot") == default(rf"{classes}\.lotml") == "LotML.Source"
    assert default(rf"{classes}\LotML.Source") == "LotML source file"
    assert default(rf"{classes}\LotML.Source\DefaultIcon") == str(icon)
    assert ".lot: LotML source file" in said and ".lotml: LotML source file" in said


def test_another_type_keeps_its_extension_unless_forced(classes, tmp_path: Path):
    set_value(rf"{classes}\.lotml", "", "Other.Type")
    done = run(tmp_path / "LotML")
    assert default(rf"{classes}\.lotml") == "Other.Type"
    assert "belongs to Other.Type" in done.stdout + done.stderr
    assert default(rf"{classes}\.lot") == "LotML.Source", "the other extension is still taken"
    run(tmp_path / "LotML", "-Force")
    assert default(rf"{classes}\.lotml") == "LotML.Source"


def test_an_extension_the_machine_gives_another_type_is_left_unless_forced(classes, tmp_path):
    set_value(rf"{MACHINE}\.lot", "", "Machine.Type")
    done = run(tmp_path / "LotML")
    assert default(rf"{classes}\.lot") is None, "no user default to shadow the machine's"
    assert "belongs to Machine.Type" in done.stdout + done.stderr
    run(tmp_path / "LotML", "-Force")
    assert default(rf"{classes}\.lot") == "LotML.Source"
    assert default(rf"{MACHINE}\.lot") == "Machine.Type", "the machine's is read, never written"


def test_remove_gives_an_extension_force_took_back_to_its_type(classes, tmp_path: Path):
    set_value(rf"{classes}\.lotml", "", "Other.Type")
    home = tmp_path / "LotML"
    run(home, "-Force")
    run(home, "-Force")
    run(home, "-Remove")
    assert default(rf"{classes}\.lotml") == "Other.Type"
    assert default(rf"{classes}\.lot") is None


def test_remove_takes_back_what_it_wrote_and_nothing_else(classes, tmp_path: Path):
    set_value(rf"{classes}\.lot", "Content Type", "text/plain")
    set_value(rf"{classes}\.lotml", "", "Other.Type")
    home = tmp_path / "LotML"
    run(home)
    run(home, "-Remove")
    assert not exists(rf"{classes}\LotML.Source")
    assert not home.exists(), "the icon's copy and its empty directory are gone"
    assert default(rf"{classes}\.lot") is None
    with winreg.OpenKey(winreg.HKEY_CURRENT_USER, rf"{classes}\.lot") as key:
        assert winreg.QueryValueEx(key, "Content Type")[0] == "text/plain", "not ours, kept"
    assert default(rf"{classes}\.lotml") == "Other.Type", "not ours, kept"
