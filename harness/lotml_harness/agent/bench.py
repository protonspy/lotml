"""The agent benchmark: one directory per task under `harness/agent_bench/`.

task.toml     kind, the graded files, optional `steps` and `seconds` limits
prompt.md     what the agent is asked
workspace/    the starting project
hidden/<f>    test blocks appended to the graded file <f> when grading
solution/     the reference solution's files, laid over workspace/
"""

import shutil
import tomllib
from dataclasses import dataclass
from pathlib import Path

from lotml_harness import ROOT

BENCH = ROOT / "harness" / "agent_bench"
KINDS = ("implement", "fix", "feature", "refactor")


@dataclass(frozen=True)
class AgentTask:
    id: str
    kind: str
    graded: tuple[str, ...]
    prompt: str
    directory: Path
    steps: int | None = None
    seconds: int | None = None

    def hidden(self, file: str) -> str:
        return (self.directory / "hidden" / file).read_text(encoding="utf-8")

    def lay(self, target: Path, solution: bool = False) -> None:
        """Copy the starting workspace to `target`, then, with `solution`, the reference over it."""
        shutil.copytree(self.directory / "workspace", target, dirs_exist_ok=True)
        if solution:
            shutil.copytree(self.directory / "solution", target, dirs_exist_ok=True)


def load(directory: Path) -> AgentTask:
    meta = tomllib.loads((directory / "task.toml").read_text(encoding="utf-8"))
    if meta["kind"] not in KINDS:
        raise ValueError(f"{directory.name}: kind {meta['kind']!r} is not one of {KINDS}")
    graded = tuple(meta["graded"])
    for file in graded:
        if not (directory / "hidden" / file).is_file():
            raise ValueError(f"{directory.name}: graded {file} has no hidden tests")
    return AgentTask(
        id=directory.name,
        kind=meta["kind"],
        graded=graded,
        prompt=(directory / "prompt.md").read_text(encoding="utf-8"),
        directory=directory,
        steps=meta.get("steps"),
        seconds=meta.get("seconds"),
    )


def tasks(root: Path = BENCH) -> list[AgentTask]:
    return [load(d) for d in sorted(root.iterdir()) if (d / "task.toml").is_file()]
