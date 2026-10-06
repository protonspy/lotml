"""The safe layer for files nobody vouches for — agent files from traces, mutants — and the
compiler run on them: a scratch copy that holds only `.lotml` files inside it, and one helper for
every `lotml` call, confined in memory, environment and time.
"""

import os
import re
import signal
import subprocess
import sys
from pathlib import Path, PurePosixPath, PureWindowsPath

from lotml_harness.execute import MEMORY, child_environment
from lotml_harness.experiments.phase1 import COMPILER

DEVICES = re.compile(r"^(con|prn|aux|nul|com[0-9¹²³]|lpt[0-9¹²³]|conin\$|conout\$)$", re.IGNORECASE)
"""Windows' reserved device names, which open a device whatever the extension or directory."""
DEADLINE = 60.0
"""Seconds one `lotml` call may take before its process tree is ended."""


def checked_name(name: str) -> str:
    """`name` when it is a relative `.lotml` path with no `..` and no reserved device name in it;
    ValueError otherwise."""
    posix, windows = PurePosixPath(name), PureWindowsPath(name)
    if posix.is_absolute() or windows.is_absolute() or windows.drive or not name:
        raise ValueError(f"{name!r} is absolute")
    if ".." in windows.parts:
        raise ValueError(f"{name!r} holds `..`")
    if windows.suffix.lower() != ".lotml" or name.rstrip() != name or name.endswith("."):
        raise ValueError(f"{name!r} is not a `.lotml` file")
    for part in windows.parts:
        if DEVICES.match(part.split(".")[0].rstrip(" ")):
            raise ValueError(f"{name!r} names a reserved device")
    return name


def lay(target: Path, files: dict[str, str]) -> list[str]:
    """Write `files` into `target`, every name checked first so nothing is written when one is
    refused; the names written, in order."""
    root = target.resolve()
    paths = {}
    for name in files:
        path = target / checked_name(name)
        if not path.resolve().is_relative_to(root):
            raise ValueError(f"{name!r} lies outside the scratch directory")
        paths[name] = path
    for name, path in paths.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(files[name], encoding="utf-8", newline="")
    return list(paths)


def lotml(
    args: list[str],
    files: list[str],
    cwd: Path,
    deadline: float = DEADLINE,
    binary: Path | str = COMPILER,
    memory: int = MEMORY,
) -> subprocess.CompletedProcess | None:
    """`lotml <args> -- <files>` in `cwd`: the clean environment, a memory cap its children share,
    and at `deadline` the whole process tree ended — a job object on Windows, a session killed as a
    group on POSIX. None when the deadline passed."""
    command = [
        sys.executable,
        "-m",
        "lotml_harness.confine",
        str(memory),
        str(binary),
        *args,
        "--",
        *files,
    ]
    posix = sys.platform != "win32"
    with subprocess.Popen(  # noqa: S603
        command,
        cwd=cwd,
        env=child_environment(),
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        encoding="utf-8",
        errors="replace",
        start_new_session=posix,
    ) as process:
        try:
            stdout, stderr = process.communicate(timeout=deadline)
        except subprocess.TimeoutExpired:
            if posix:
                os.killpg(process.pid, signal.SIGKILL)
            else:
                process.kill()
            process.communicate()
            return None
    return subprocess.CompletedProcess(command, process.returncode, stdout, stderr)
