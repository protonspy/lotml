"""Whether each module of the binding coverage corpus is imported and run with no `lotml bind`
(specs/bind-on-import R4.1): one program per module, `import py.<module>`, run with `lotml run` and
no `bindings/` directory, the PyPI modules in one scratch project whose `uv.lock` pins them.

The harness installs nothing while it measures (adr:0033): `--provision` locks the scratch project
and makes its environment once, online; a measurement then runs offline from that environment, in
the cache of its own the scratch project keeps.

    python -m lotml_harness.experiments.bind_on_import --provision
    python -m lotml_harness.experiments.bind_on_import
"""

import argparse
import os
import re
import shutil
import subprocess
from dataclasses import dataclass
from pathlib import Path

from lotml_harness import ROOT
from lotml_harness.execute import child_environment
from lotml_harness.experiments.binding_coverage import CORPUS, Module, corpus
from lotml_harness.experiments.phase1 import COMPILER, RESULTS

SCRATCH = ROOT / "harness" / "cache" / "bind-on-import"
"""The scratch project, git-ignored, kept between runs so its environment is made once."""
REPORT = RESULTS / "bind-on-import.md"
TIMEOUT = 300
EXCLUDE_NEWER = "2026-10-09T00:00:00Z"
"""No release after this enters the lock: the runtime distributions are not pinned by the harness,
and a release published later is code the measurement would run unreviewed."""
PYPROJECT = ROOT / "harness" / "pyproject.toml"


@dataclass
class Outcome:
    module: str
    group: str
    passed: bool
    said: str = ""
    """The first line the failure printed; empty when it passed."""


def runtime(distribution: str) -> str:
    """The distribution that installs the module a stub distribution types: `types-X` and
    `X-stubs` type `X`; any other distribution is its own."""
    if distribution.lower().startswith("types-"):
        return distribution[len("types-") :]
    if distribution.lower().endswith("-stubs"):
        return distribution[: -len("-stubs")]
    return distribution


def pins(text: str) -> dict[str, str]:
    """The pinned requirements of the harness group `stubs`, by distribution, lowercased."""
    group = text[text.index("stubs = [") :].split("]")[0]
    found = re.findall(r'"([A-Za-z0-9_.\-]+)==([^"]+)"', group)
    return {name.lower(): f"{name}=={version}" for name, version in found}


def pyproject(modules: list[Module], pinned: dict[str, str]) -> str:
    """The scratch project's manifest: each PyPI module's runtime distribution, and its stub
    distribution pinned as the binding coverage report reads it."""
    wanted = set()
    for m in modules:
        if m.group != "pypi":
            continue
        wanted.add(pinned.get(m.distribution.lower(), m.distribution))
        package = runtime(m.distribution)
        wanted.add(pinned.get(package.lower(), package))
    listed = "".join(f'    "{w}",\n' for w in sorted(wanted, key=str.lower))
    return (
        '[project]\nname = "bind-on-import"\nversion = "0.1.0"\nrequires-python = ">=3.12"\n'
        f"dependencies = [\n{listed}]\n"
    )


def program(module: str) -> str:
    """A program that imports `module` by its origin and runs."""
    return f'import py.{module}\n\nfn main():\n    print("ran")\n'


HOMES = ("USERPROFILE", "LOCALAPPDATA", "HOME", "XDG_CACHE_HOME")
"""Where the user's directories are, which lotml checks its cache lies under on Windows."""


def environment(online: bool = False) -> dict[str, str]:
    """A child's environment, with the scratch project's own lotml cache; offline unless asked."""
    homes = {k: v for k in HOMES if (v := os.environ.get(k))}
    env = child_environment() | homes | {"LOTML_CACHE_DIR": str(SCRATCH / "lotml-cache")}
    if online:
        env.pop("LOTML_OFFLINE", None)
    return env


def run(compiler: Path, project: Path, module: Module, env: dict[str, str]) -> Outcome:
    """`lotml run` of the program importing `module`, in `project`."""
    source = project / f"{module.name.replace('.', '_')}.lotml"
    source.write_text(program(module.name), encoding="utf-8", newline="\n")
    try:
        ran = subprocess.run(  # noqa: S603 - the compiler, with fixed arguments
            [str(compiler), "run", source.name],
            cwd=project,
            capture_output=True,
            text=True,
            encoding="utf-8",
            env=env,
            timeout=TIMEOUT,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return Outcome(module.name, module.group, False, f"no answer in {TIMEOUT} s")
    if ran.returncode == 0 and ran.stdout.strip() == "ran":
        return Outcome(module.name, module.group, True)
    lines = [line for line in (ran.stdout + ran.stderr).splitlines() if line.strip()]
    return Outcome(
        module.name, module.group, False, lines[0] if lines else f"exit {ran.returncode}"
    )


def prepare(project: Path, modules: list[Module]) -> None:
    """The scratch project's directory, manifest and `.git` mark, with no `bindings/`."""
    project.mkdir(parents=True, exist_ok=True)
    (project / ".git").touch()
    (project / "pyproject.toml").write_text(
        pyproject(modules, pins(PYPROJECT.read_text(encoding="utf-8"))),
        encoding="utf-8",
        newline="\n",
    )
    shutil.rmtree(project / "bindings", ignore_errors=True)


def provision(compiler: Path, modules: list[Module]) -> None:
    """Lock the scratch project and make its environment, online, once."""
    project = SCRATCH / "pypi"
    prepare(project, modules)
    uv = shutil.which("uv")
    if uv is None:
        raise SystemExit("locking the scratch project needs uv on the path")
    subprocess.run(  # noqa: S603 - uv, with fixed arguments
        [uv, "lock", "--exclude-newer", EXCLUDE_NEWER],
        cwd=project,
        env=environment(online=True),
        check=True,
    )
    first = next(m for m in modules if m.group == "pypi")
    source = project / "provision.lotml"
    source.write_text(program(first.name), encoding="utf-8", newline="\n")
    made = subprocess.run(  # noqa: S603 - the compiler, with fixed arguments
        [str(compiler), "run", source.name],
        cwd=project,
        capture_output=True,
        text=True,
        encoding="utf-8",
        env=environment(online=True),
        check=False,
    )
    if made.returncode != 0:
        raise SystemExit(f"the environment was not made:\n{made.stdout}{made.stderr}")
    print(f"provisioned {project}")


def measure(compiler: Path, modules: list[Module]) -> list[Outcome]:
    """Each module run offline: the standard library's in a project of their own, the PyPI ones
    in the provisioned one."""
    stdlib, pypi = SCRATCH / "stdlib", SCRATCH / "pypi"
    stdlib.mkdir(parents=True, exist_ok=True)
    (stdlib / ".git").touch()
    shutil.rmtree(stdlib / "bindings", ignore_errors=True)
    if not (pypi / "uv.lock").is_file():
        raise SystemExit("run with --provision first: the PyPI modules need the locked project")
    shutil.rmtree(pypi / "bindings", ignore_errors=True)
    env = environment()
    return [run(compiler, stdlib if m.group == "stdlib" else pypi, m, env) for m in modules]


def markdown(outcomes: list[Outcome]) -> str:
    passed = sum(o.passed for o in outcomes)
    lines = [
        "# Binding on import: the corpus run with no `lotml bind`",
        "",
        "Generated by `python -m lotml_harness.experiments.bind_on_import`. Each module of",
        "`harness/binding-corpus.json` imported as `py.<module>` by a program run with `lotml run`",
        "and no `bindings/` directory, offline; the PyPI modules in a scratch project whose",
        "`uv.lock` pins them (specs/bind-on-import R4.1). `lotml bind <module>` says why one",
        "fails.",
        "",
        f"{passed} of {len(outcomes)} modules ran.",
        "",
        "| module | group | result |",
        "| --- | --- | --- |",
    ]
    for o in outcomes:
        said = o.said.replace("|", "\\|")
        lines.append(f"| {o.module} | {o.group} | {'ran' if o.passed else f'failed: {said}'} |")
    return "\n".join(lines) + "\n"


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--compiler", type=Path, default=COMPILER, help="the lotml to measure")
    parser.add_argument("--provision", action="store_true", help="lock and install, online, once")
    args = parser.parse_args(argv)
    modules = corpus(CORPUS.read_text(encoding="utf-8"))
    if args.provision:
        provision(args.compiler, modules)
        return
    outcomes = measure(args.compiler, modules)
    REPORT.write_text(markdown(outcomes), encoding="utf-8", newline="\n")
    print(markdown(outcomes).split("\n\n")[2])


if __name__ == "__main__":
    main()
