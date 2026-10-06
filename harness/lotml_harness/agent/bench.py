"""The agent benchmark: one directory per task under `harness/agent_bench/`.

task.toml     kind, the graded files, optional `steps` and `seconds` limits
prompt.md     what the agent is asked
workspace/    the starting project
hidden/<f>    test blocks appended to the graded file <f> when grading
solution/     the reference solution's files, laid over workspace/
"""

import tomllib
from dataclasses import dataclass, field
from pathlib import Path, PurePosixPath, PureWindowsPath

from lotml_harness import ROOT

BENCH = ROOT / "harness" / "agent_bench"
KINDS = ("implement", "fix", "feature", "refactor")


@dataclass(frozen=True)
class AgentTask:
    """A task and its files as data, by name relative to the workspace. Holding dictionaries, it is
    never hashed."""

    id: str
    kind: str
    graded: tuple[str, ...]
    prompt: str
    workspace_files: dict[str, str]
    hidden_files: dict[str, str]
    solution_files: dict[str, str] = field(default_factory=dict)
    directory: Path | None = None
    steps: int | None = None
    seconds: int | None = None

    def hidden(self, file: str) -> str:
        return self.hidden_files[file]

    def lay(self, target: Path, solution: bool = False) -> None:
        """Write the starting workspace into `target`, then, with `solution`, the reference over
        it. A name that is absolute, holds `..` or resolves outside `target` is refused."""
        files = self.workspace_files | (self.solution_files if solution else {})
        paths = {name: inside(target, name) for name in files}
        for name, path in paths.items():
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(files[name], encoding="utf-8", newline="")


def inside(target: Path, name: str) -> Path:
    """`target / name`, or ValueError when `name` could put the file anywhere else."""
    posix, windows = PurePosixPath(name), PureWindowsPath(name)
    if posix.is_absolute() or windows.is_absolute() or windows.drive or ".." in windows.parts:
        raise ValueError(f"{name!r} lies outside the workspace")
    path = target / name
    if not path.resolve().is_relative_to(target.resolve()):
        raise ValueError(f"{name!r} lies outside the workspace")
    return path


def files(directory: Path) -> dict[str, str]:
    """Every regular file under `directory` by its relative name, symbolic links skipped."""
    if not directory.is_dir():
        return {}
    return {
        path.relative_to(directory).as_posix(): path.read_text(encoding="utf-8")
        for path in sorted(directory.rglob("*"))
        if path.is_file() and not path.is_symlink()
    }


def load(directory: Path) -> AgentTask:
    meta = tomllib.loads((directory / "task.toml").read_text(encoding="utf-8"))
    if meta["kind"] not in KINDS:
        raise ValueError(f"{directory.name}: kind {meta['kind']!r} is not one of {KINDS}")
    graded = tuple(meta["graded"])
    hidden = files(directory / "hidden")
    for file in graded:
        if file not in hidden:
            raise ValueError(f"{directory.name}: graded {file} has no hidden tests")
    return AgentTask(
        id=directory.name,
        kind=meta["kind"],
        graded=graded,
        prompt=(directory / "prompt.md").read_text(encoding="utf-8"),
        workspace_files=files(directory / "workspace"),
        hidden_files=hidden,
        solution_files=files(directory / "solution"),
        directory=directory,
        steps=meta.get("steps"),
        seconds=meta.get("seconds"),
    )


def tasks(root: Path = BENCH) -> list[AgentTask]:
    return [load(d) for d in sorted(root.iterdir()) if (d / "task.toml").is_file()]
