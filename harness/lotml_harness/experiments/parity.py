"""The parity suite: every program of the corpus runs its `test` blocks on the Python target and
on the LLVM target, and the two must report the same (specs/llvm-parity R1.4, R5.1;
docs/wiki/pages/evaluation-harness.md). Each run is recorded in
`results/parity-llvm.md`, beside the retired C target's last run in `results/parity.md`
(adr:0025-two-targets-python-for-run-llvm-for-build).

A program that imports a Python module is set apart: a native program runs without Python, so the
LLVM target refuses it at the import; it runs under `lotml run`.

The suite is a gate through its floor (plans/target-parity-assurance.md):
`results/parity/floor.json` names the programs that report the same and their count. A run fails
the floor when a program leaves it or the count drops; a program that joins it only asks for an
update, made from a whole run and never from one `--only` filtered. A program leaves the floor
itself only with a reason, which the update records (`--reason`) and which `check --since <ref>`
asks of every program the floor at `<ref>` held and this one does not.

    python -m lotml_harness.experiments.parity                # the suite, and its report
    python -m lotml_harness.experiments.parity floor check    # fails when a program left it
    python -m lotml_harness.experiments.parity floor check --since origin/main
    python -m lotml_harness.experiments.parity floor diff     # what left it and what joined
    python -m lotml_harness.experiments.parity floor update   # the floor, from a whole run
    python -m lotml_harness.experiments.parity floor update --reason "why"   # one may leave it

Each command takes `--only <prefix>` to run the programs whose task starts with it.
"""

import argparse
import collections
import concurrent.futures
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path

from lotml_harness import ROOT
from lotml_harness.corpus.pipeline import CORPUS
from lotml_harness.experiments.gate2 import compiler
from lotml_harness.experiments.phase1 import RESULTS

REPORT = RESULTS / "parity-llvm.md"
RUNS = RESULTS / "parity" / "parity-llvm.jsonl"
FLOOR = RESULTS / "parity" / "floor.json"
UNSUPPORTED = RESULTS / "parity" / "unsupported.json"
WORKERS = 8
"""Programs built and run at once: each native build is a process of its own."""
SHOWN = 20
"""Programs a report names under each verdict that is not `same`."""
NATIVE_BUILD = (
    "PATH",
    "SYSTEMROOT",
    "SYSTEMDRIVE",
    "WINDIR",
    "TEMP",
    "TMP",
    "TMPDIR",
    "COMSPEC",
    "PATHEXT",
    "PROGRAMFILES",
    "PROGRAMFILES(X86)",
    "PROGRAMDATA",
    "INCLUDE",
    "LIB",
    "LIBPATH",
    "VCINSTALLDIR",
    "VSINSTALLDIR",
    "WINDOWSSDKDIR",
    "LOTML_CLANG",
    "LOTML_CACHE_DIR",
    "LOCALAPPDATA",
    "USERPROFILE",
    "HOME",
    "XDG_CACHE_HOME",
)
"""All a native build and the program it runs get from the environment: what finds `clang`, its
temporary files and, on Windows, Visual Studio's linker and the SDK, and the per-user cache of the
runtime's object. No other variable reaches the program by accident."""


@dataclass
class Outcome:
    task: str
    verdict: str
    """`same`, `differs`, `refused` (a Python import), `not compiled` or `no report`."""
    detail: str


@dataclass
class Floor:
    count: int
    """How many programs report the same."""
    same: list[str]
    """Which, sorted."""
    left: dict[str, str] = field(default_factory=dict)
    """The programs taken out of the floor, each with why."""


def native_environment() -> dict[str, str]:
    """The environment of `lotml test --target llvm`: `NATIVE_BUILD` and nothing else."""
    return {k: v for k, v in os.environ.items() if k.upper() in NATIVE_BUILD}


def tests_of(ran: subprocess.CompletedProcess) -> list[dict] | None:
    """The rows of a `lotml test --json` report, without the file each came from."""
    try:
        report = json.loads(ran.stdout.strip().splitlines()[-1])
    except (json.JSONDecodeError, IndexError):
        return None
    return [{k: v for k, v in row.items() if k != "file"} for row in report.get("tests", [])]


def compare(
    task: str, python: subprocess.CompletedProcess, native: subprocess.CompletedProcess
) -> Outcome:
    """What the LLVM target's report of one program is next to the Python target's."""
    expected, found = tests_of(python), tests_of(native)
    if expected is None:
        return Outcome(task, "no report", "the Python target: " + python.stdout[-300:])
    if found is None:
        first = re.search(r"error\[E\d+\]: [^\n]*", native.stdout)
        if first and first.group(0).startswith("error[E0401]"):
            return Outcome(task, "refused", first.group(0))
        if first:
            return Outcome(task, "not compiled", first.group(0))
        return Outcome(task, "no report", (native.stdout + native.stderr)[-300:])
    if found == expected:
        return Outcome(task, "same", "")
    pairs = [(a, b) for a, b in zip(expected, found, strict=False) if a != b]
    if len(expected) != len(found):
        pairs.append(({"tests": len(expected)}, {"tests": len(found)}))
    a, b = pairs[0]
    return Outcome(task, "differs", f"Python {json.dumps(a)}; LLVM {json.dumps(b)}")


def run_both(entry: dict, root: Path) -> Outcome:
    name = re.sub(r"\W", "_", entry["task"]) + ".lotml"
    (root / name).write_text(entry["lotml"], encoding="utf-8")
    python = compiler(["test", "--json", name], root)
    native = compiler(["test", "--json", "--target", "llvm", name], root, env=native_environment())
    return compare(entry["task"], python, native)


def suite(entries: list[dict]) -> list[Outcome]:
    """Each program's tests on both targets, in the order of `entries`."""
    with tempfile.TemporaryDirectory(prefix="lotml-parity-") as directory:
        root = Path(directory)
        with concurrent.futures.ThreadPoolExecutor(WORKERS) as pool:
            return list(pool.map(lambda e: run_both(e, root), entries))


def floor_of(outcomes: list[Outcome]) -> Floor:
    same = sorted(o.task for o in outcomes if o.verdict == "same")
    return Floor(len(same), same)


def updated(old: Floor, outcomes: list[Outcome], reason: str | None) -> Floor:
    """The floor of a whole run. A program of `old` the run lost leaves it only with `reason`,
    recorded beside it; one that reports the same again rejoins it."""
    new = floor_of(outcomes)
    lost = sorted(set(old.same) - set(new.same))
    if lost and not (reason and reason.strip()):
        raise SystemExit(
            f"{len(lost)} programs would leave the floor ({', '.join(lost[:10])}): "
            "give the reason with --reason, or fix them"
        )
    left = {t: why for t, why in old.left.items() if t not in new.same}
    left |= {t: reason.strip() for t in lost if reason is not None}
    return Floor(new.count, new.same, dict(sorted(left.items())))


def unexplained(before: Floor, now: Floor) -> list[str]:
    """The programs `before` held that `now` does not, and gives no reason for."""
    return [
        f"`{t}` was taken out of the floor without a recorded reason"
        for t in sorted(set(before.same) - set(now.same))
        if not now.left.get(t, "").strip()
    ]


def problems(floor: Floor, outcomes: list[Outcome]) -> list[str]:
    """Why a run fails the floor: a program of it that does not report the same, or fewer
    programs reporting the same than it holds. A program that joined it is no problem."""
    now = {o.task: o for o in outcomes}
    found = []
    for task in floor.same:
        outcome = now.get(task)
        if outcome is None:
            found.append(f"`{task}` did not run")
        elif outcome.verdict != "same":
            found.append(f"`{task}` left the floor: {outcome.verdict} {outcome.detail}".rstrip())
    count = sum(o.verdict == "same" for o in outcomes)
    if count < floor.count:
        found.append(f"{count} programs report the same, fewer than the floor's {floor.count}")
    return found


def diff(floor: Floor, outcomes: list[Outcome]) -> tuple[list[str], list[str]]:
    """The programs that left the floor and the ones that joined it."""
    now = set(floor_of(outcomes).same)
    return sorted(set(floor.same) - now), sorted(now - set(floor.same))


def dump_floor(floor: Floor) -> str:
    data = {"count": floor.count, "same": floor.same, "left": floor.left}
    return json.dumps(data, indent=1) + "\n"


def load_floor(text: str) -> Floor:
    data = json.loads(text)
    return Floor(data["count"], data["same"], data.get("left", {}))


def floor_at(ref: str) -> Floor:
    """The floor as committed at the git revision `ref`; an empty one before the floor existed."""
    if ref.startswith("-"):
        raise SystemExit(f"`{ref}` is not a revision")
    git = shutil.which("git")
    if git is None:
        raise SystemExit("git is not on PATH: --since reads the floor at a revision")

    def run(*args: str) -> subprocess.CompletedProcess:
        return subprocess.run(  # noqa: S603 - git, on this repository
            [git, *args], cwd=ROOT, capture_output=True, text=True, encoding="utf-8", check=False
        )

    if run("rev-parse", "--verify", "--quiet", "--end-of-options", f"{ref}^{{commit}}").returncode:
        raise SystemExit(f"cannot read the floor at {ref}: no such revision")
    blob = f"{ref}:{FLOOR.relative_to(ROOT).as_posix()}"
    if run("cat-file", "-e", blob).returncode:
        return Floor(0, [])
    return load_floor(run("show", blob).stdout)


def unsupported(outcomes: list[Outcome]) -> dict[str, dict[str, list[str]]]:
    """The programs refused or not compiled, grouped by verdict and by the message that stopped
    each, so a missing feature is counted once rather than found program by program."""
    grouped: dict[str, dict[str, list[str]]] = {"refused": {}, "not compiled": {}}
    for o in outcomes:
        if o.verdict in grouped:
            grouped[o.verdict].setdefault(o.detail, []).append(o.task)
    return {
        verdict: {message: sorted(tasks) for message, tasks in sorted(groups.items())}
        for verdict, groups in grouped.items()
    }


def markdown(outcomes: list[Outcome]) -> str:
    counts = collections.Counter(o.verdict for o in outcomes)
    ran = [o for o in outcomes if o.verdict != "refused"]
    passed = bool(ran) and all(o.verdict == "same" for o in ran)
    lines = [
        "# The suite on the Python and LLVM targets",
        "",
        "Generated by `python -m lotml_harness.experiments.parity`. Every program of the corpus",
        "runs its `test` blocks with `lotml test --json` on the Python target and with",
        "`--target llvm`, and the two reports must hold the same rows: each test's outcome and,",
        "for one that fails, the values it compared, its line, and what stopped it. A program",
        "that imports a Python module is refused by the LLVM target at the import, as a native",
        "program runs without Python (adr:0025), and set apart.",
        "",
        "| verdict | programs |",
        "| --- | ---: |",
    ]
    for verdict in ("same", "differs", "not compiled", "no report", "refused"):
        lines.append(f"| {verdict} | {counts.get(verdict, 0)} |")
    lines += ["", f"{len(ran)} programs ran on both targets.", ""]
    for verdict in ("differs", "not compiled", "no report"):
        named = [o for o in outcomes if o.verdict == verdict]
        if named:
            lines += [f"## {verdict.capitalize()}", ""]
            lines += [f"- `{o.task}`: {o.detail}" for o in named[:SHOWN]]
            if len(named) > SHOWN:
                lines.append(
                    f"- and {len(named) - SHOWN} more, in `results/parity/parity-llvm.jsonl`"
                )
            lines.append("")
    lines += [f"The suite {'passes' if passed else 'does not pass'} on both targets.", ""]
    return "\n".join(lines)


def entries(only: str | None) -> list[dict]:
    rows = [json.loads(line) for line in CORPUS.read_text(encoding="utf-8").splitlines()]
    return [r for r in rows if only is None or r["task"].startswith(only)]


def write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8", newline="\n")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="python -m lotml_harness.experiments.parity")
    parser.add_argument("command", nargs="*", help="nothing, or `floor check|diff|update`")
    parser.add_argument("--only", help="run only the programs whose task starts with this")
    parser.add_argument(
        "--reason", help="`floor update`: why the programs it drops leave the floor"
    )
    parser.add_argument("--since", help="`floor check`: also hold the floor to the one at this ref")
    args = parser.parse_args(argv)
    if args.command and (
        args.command[0] != "floor" or args.command[1:] not in (["check"], ["diff"], ["update"])
    ):
        parser.error("the commands are `floor check`, `floor diff` and `floor update`")
    action = args.command[1] if args.command else None
    if action == "update" and args.only is not None:
        raise SystemExit("the floor is updated from a whole run, never from one filtered by --only")
    outcomes = suite(entries(args.only))
    if action is None:
        write(RUNS, "".join(json.dumps(o.__dict__) + "\n" for o in outcomes))
        write(UNSUPPORTED, json.dumps(unsupported(outcomes), indent=1) + "\n")
        report = markdown(outcomes)
        write(REPORT, report)
        sys.stdout.buffer.write(report.encode("utf-8"))
        return 0
    if action == "update":
        old = load_floor(FLOOR.read_text(encoding="utf-8")) if FLOOR.is_file() else Floor(0, [])
        floor = updated(old, outcomes, args.reason)
        write(FLOOR, dump_floor(floor))
        print(f"the floor holds {floor.count} programs")
        return 0
    floor = load_floor(FLOOR.read_text(encoding="utf-8"))
    if floor.count != len(floor.same):
        raise SystemExit(f"the floor counts {floor.count} programs and names {len(floor.same)}")
    unrecorded = unexplained(floor_at(args.since), floor) if args.since else []
    if args.only is not None:
        floor = Floor(floor.count, [t for t in floor.same if t.startswith(args.only)])
        floor.count = len(floor.same)
    if action == "diff":
        left, joined = diff(floor, outcomes)
        print(f"left the floor ({len(left)}): " + ", ".join(left))
        print(f"joined it ({len(joined)}): " + ", ".join(joined))
        return 0
    found = unrecorded + problems(floor, outcomes)
    for problem in found:
        print(problem)
    if found:
        return 1
    print(f"the floor holds: {floor.count} programs report the same")
    return 0


if __name__ == "__main__":
    sys.exit(main())
