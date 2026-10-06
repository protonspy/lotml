"""The safe layer for files nobody vouches for — agent files from traces, mutants — and the
compiler run on them: a scratch copy that holds only `.lotml` files inside it, and one helper for
every `lotml` call, confined in memory, environment, time and output.
"""

import os
import re
import signal
import subprocess
import sys
import tempfile
from pathlib import Path, PurePosixPath, PureWindowsPath

from lotml_harness.execute import MEMORY, child_environment
from lotml_harness.experiments.phase1 import COMPILER

DEVICES = re.compile(r"^(con|prn|aux|nul|com[0-9¹²³]|lpt[0-9¹²³]|conin\$|conout\$)$", re.IGNORECASE)
"""Windows' reserved device names, which open a device whatever the extension or directory."""
DEADLINE = 60.0
"""Seconds one `lotml` call may take before its process tree is ended."""
FILES = 256
"""Files one scratch copy may hold."""
FILE_BYTES = 2**20
TOTAL_BYTES = 4 * 2**20
"""Bytes one file, and all of a copy's files, may hold: agent files are kilobytes."""
OUTPUT = 32 * 2**20
"""Bytes of a call's output read back; a call that prints more gave no report."""


def checked_name(name: str) -> str:
    """`name` when it is a relative `.lotml` path with no `..`, no `:` and no reserved device name
    in it; ValueError otherwise."""
    if not isinstance(name, str) or not name:
        raise ValueError(f"{name!r} is not a file name")
    posix, windows = PurePosixPath(name), PureWindowsPath(name)
    if posix.is_absolute() or windows.is_absolute() or windows.drive:
        raise ValueError(f"{name!r} is absolute")
    if ":" in name:
        raise ValueError(f"{name!r} names a stream or a drive")
    if ".." in windows.parts:
        raise ValueError(f"{name!r} holds `..`")
    if windows.suffix.lower() != ".lotml" or name.rstrip() != name or name.endswith("."):
        raise ValueError(f"{name!r} is not a `.lotml` file")
    for part in windows.parts:
        if DEVICES.match(part.split(".")[0].rstrip(" ")):
            raise ValueError(f"{name!r} names a reserved device")
    return name


def lay(target: Path, files: dict[str, str]) -> list[str]:
    """Write `files` into `target`, every name and size checked first so nothing is written when
    one is refused; the names written, in order. An empty `.git` at the root stops the compiler's
    search for interfaces inside the copy, so no `bindings/` above it applies."""
    if len(files) > FILES:
        raise ValueError(f"{len(files)} files are more than {FILES}")
    root = target.resolve()
    paths, total = {}, 0
    for name, text in files.items():
        path = target / checked_name(name)
        if not path.resolve().is_relative_to(root):
            raise ValueError(f"{name!r} lies outside the scratch directory")
        if not isinstance(text, str):
            raise ValueError(f"{name!r} holds no text")
        size = len(text.encode("utf-8"))
        total += size
        if size > FILE_BYTES or total > TOTAL_BYTES:
            raise ValueError(f"{name!r} takes the copy past its size")
        paths[name] = path
    (target / ".git").mkdir(parents=True, exist_ok=True)
    for name, path in paths.items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(files[name], encoding="utf-8", newline="")
    return list(paths)


def _read(file, limit: int) -> str | None:
    file.seek(0)
    data = file.read(limit + 1)
    return None if len(data) > limit else data.decode("utf-8", "replace")


def lotml(
    args: list[str],
    files: list[str],
    cwd: Path,
    deadline: float = DEADLINE,
    binary: Path | str = COMPILER,
    memory: int = MEMORY,
    output: int = OUTPUT,
) -> subprocess.CompletedProcess | None:
    """`lotml <args> -- <files>` in `cwd`: the clean environment, a memory cap on the call and on
    each process it starts, its output written to files and only `output` bytes of it read, and
    at `deadline` the whole process tree ended — a job object on Windows, a session killed as a
    group on POSIX. None when the deadline passed; a call that printed more than `output` bytes
    comes back with no output and status -1."""
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
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        with subprocess.Popen(  # noqa: S603
            command,
            cwd=cwd,
            env=child_environment(),
            stdout=out,
            stderr=err,
            start_new_session=posix,
        ) as process:
            try:
                process.wait(timeout=deadline)
            except subprocess.TimeoutExpired:
                try:
                    if posix:
                        os.killpg(process.pid, signal.SIGKILL)
                    else:
                        process.kill()
                except ProcessLookupError:
                    pass
                process.wait()
                return None
        stdout, stderr = _read(out, output), _read(err, output)
    if stdout is None or stderr is None:
        return subprocess.CompletedProcess(command, -1, "", f"the output passed {output} bytes")
    return subprocess.CompletedProcess(command, process.returncode, stdout, stderr)
