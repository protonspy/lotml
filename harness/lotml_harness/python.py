"""The CPython the harness's children run the Python target on (adr:0026,
plans/python-via-uv.md 3.1): CPython 3.14, provisioned once and ahead of a run through the uv lotml
finds, by

    python -m lotml_harness.python --provision

A run itself is offline (`child_environment`): a child that finds no interpreter fails naming
what is missing, and downloads nothing.
"""

import argparse
import json
import subprocess
import tempfile
from pathlib import Path

from lotml_harness.execute import child_environment
from lotml_harness.experiments.phase1 import COMPILER

PROBE = "fn main():\n    print(1)\n"
"""A program that runs on any interpreter: what `lotml run` resolves one for."""

TIMEOUT = 600
"""Seconds a provisioning run may take, its download included."""


def interpreter(compiler: Path = COMPILER, online: bool = False) -> dict | None:
    """The interpreter `lotml run` resolves for a child, as its JSON records it: path, version
    and uv's version; none when it finds none. Online, it is allowed to download CPython 3.14."""
    env = child_environment()
    if online:
        env.pop("LOTML_OFFLINE", None)
    with tempfile.TemporaryDirectory(prefix="lotml-python-") as scratch:
        probe = Path(scratch) / "probe.lotml"
        probe.write_text(PROBE, encoding="utf-8", newline="\n")
        ran = subprocess.run(  # noqa: S603 - the compiler, with fixed arguments
            [str(compiler), "run", "--json", str(probe)],
            capture_output=True,
            text=True,
            encoding="utf-8",
            env=env,
            cwd=scratch,
            timeout=TIMEOUT,
            check=False,
        )
    for line in reversed(ran.stdout.splitlines()):
        if line.startswith("{"):
            report = json.loads(line)
            if ran.returncode == 0 and isinstance(report.get("python"), dict):
                return report["python"]
            break
    return None


def provision(compiler: Path = COMPILER) -> dict:
    """CPython 3.14 made available to every later run, downloaded through lotml's uv if it is not
    there yet: the one step of the harness that may reach the network."""
    found = interpreter(compiler, online=True)
    if found is None:
        raise SystemExit("lotml found no Python, even allowed to download CPython 3.14 through uv")
    return found


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument(
        "--provision", action="store_true", help="download CPython 3.14, online, once"
    )
    parser.add_argument("--compiler", type=Path, default=COMPILER)
    args = parser.parse_args()
    found = provision(args.compiler) if args.provision else interpreter(args.compiler)
    if found is None:
        raise SystemExit(
            "no Python for the harness's runs: run `python -m lotml_harness.python --provision`"
        )
    print(json.dumps(found))


if __name__ == "__main__":
    main()
