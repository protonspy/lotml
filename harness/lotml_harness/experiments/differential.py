"""The two-target differential fuzzer (plans/target-parity-assurance.md, tasks 2.1 to 2.3;
docs/wiki/pages/target-parity.md, "Fuzzing one target against the other"). Typed programs are
generated from templates, and mutants are taken from the corpus with `lotml dev mutate`; each runs
on the Python target, the oracle, and on the LLVM target, and the two must agree on what the
program printed, on how it ended and, when it panicked, on the kind of error — never on the bytes
of the message. Many generated cases go into one `main`, as a native build costs more than a case;
a panic ends the program, so the cases before it are compared and the one that stopped it is named.

A case that differs is minimized, deleting chunks greedily while it still differs, and written to
`results/parity/fuzz/` as a parity program. Native programs are built with `LOTML_SANITIZE` and
with their cells counted, so a counting bug shows even when the output agrees.

Each run gets a wall-clock limit and a working directory of its own, thrown away after it; the
templates generate no import and no file access.

    python -m lotml_harness.experiments.differential [--seed N] [--batches N] [--mutants N]
"""

import argparse
import json
import os
import random
import re
import shutil
import subprocess
import sys
import tempfile
from collections.abc import Callable
from dataclasses import dataclass
from pathlib import Path

from lotml_harness.corpus.pipeline import CORPUS
from lotml_harness.execute import child_environment
from lotml_harness.experiments import parity
from lotml_harness.experiments.gate2 import compiler
from lotml_harness.experiments.phase1 import RESULTS

REPORT = RESULTS / "parity" / "differential.md"
FOUND = RESULTS / "parity" / "fuzz"
TIMEOUT = 60
"""Seconds a program has to build and run on one target."""
CASES = 40
"""Generated cases in one program."""
PANIC = re.compile(r"^panic: ([A-Za-z]+)", re.MULTILINE)
LIVE = re.compile(r"lotml: (-?\d+) cells live at exit")
STOPPED = re.compile(r"in case_(\d+)")

INTS = [0, 1, -1, 2, -2, 3, 7, -7, 10, 255, 1000, 2**31, -(2**31), 2**62, 2**63 - 1, -(2**63)]
FLOATS = ["0.0", "-0.0", "1.0", "-1.5", "0.1", "2.5", "3.5", "1e-7", "1e16", "1e300", "-1e300"]
TEXTS = ['""', '"a"', '"abc"', '"héllo"', '"日本語"', '"a b  c"', '"0123456789"', '"Ab,cD"']
INT_OPERATORS = ["+", "-", "*", "//", "%", "**", "<<", ">>", "&", "|", "^", "/"]
FLOAT_OPERATORS = ["+", "-", "*", "/", "//", "%", "**"]
COMPARISONS = ["<", "<=", "==", "!=", ">", ">="]
INT_SPECS = ["", ">6", "<6", "^7", "06", "+d", ",", "_x", "#x", "b", "o", "X", "e", ".2%"]
FLOAT_SPECS = ["", ".2f", ".3e", "g", ".1%", "+.1f", ">10.3f", ",.2f", "z.1f", ".0f"]
TEXT_SPECS = ["", ">5", "<5", "^6", "*^9", ".2", "s"]


@dataclass
class Run:
    stdout: str
    status: int | None
    """The exit status, or None when the run outlived its limit."""
    kind: str
    """The kind of error a panic named, or empty."""
    live: int | None
    """The cells a counting native build left at exit, when it said."""
    stopped: int | None = None
    """The case whose panic ended the program, when its traceback names one."""


@dataclass
class Verdict:
    same: bool
    detail: str


def _int(rng: random.Random) -> str:
    value = rng.choice(INTS) if rng.random() < 0.7 else rng.randint(-1000, 1000)
    return f"({value})" if value < 0 else str(value)


def _float(rng: random.Random) -> str:
    text = rng.choice(FLOATS)
    return f"({text})" if text.startswith("-") else text


def _text(rng: random.Random) -> str:
    return rng.choice(TEXTS)


def _ints(rng: random.Random, low: int, high: int) -> str:
    return "[" + ", ".join(_int(rng) for _ in range(rng.randint(low, high))) + "]"


def _floats(rng: random.Random, low: int, high: int) -> str:
    return "[" + ", ".join(_float(rng) for _ in range(rng.randint(low, high))) + "]"


def _slice(r: random.Random) -> str:
    return f"[{r.randint(-3, 3)}:{r.randint(-2, 6)}]"


def _rounds(r: random.Random) -> str:
    return f"round({_float(r)}), round({_float(r)}, {r.randint(0, 3)})"


def _chain(r: random.Random) -> str:
    return f"{_int(r)} {r.choice(COMPARISONS)} {_int(r)} {r.choice(COMPARISONS)} {_int(r)}"


TEMPLATES: list[Callable[[random.Random], str]] = [
    lambda r: f"print({_int(r)} {r.choice(INT_OPERATORS)} {_int(r)})",
    lambda r: f"print({_float(r)} {r.choice(FLOAT_OPERATORS)} {_float(r)})",
    lambda r: f"print({_int(r)} {r.choice(FLOAT_OPERATORS)} {_float(r)})",
    lambda r: f"print({_chain(r)})",
    lambda r: f"print({_float(r)} {r.choice(COMPARISONS)} {_float(r)})",
    lambda r: f'print(f"[{{{_int(r)}:{r.choice(INT_SPECS)}}}]")',
    lambda r: f'print(f"[{{{_float(r)}:{r.choice(FLOAT_SPECS)}}}]")',
    lambda r: f"print(f'[{{{_text(r)}:{r.choice(TEXT_SPECS)}}}]')",
    lambda r: f"print({_text(r)}[{r.randint(-4, 4)}])",
    lambda r: f"print({_text(r)}[{r.randint(-4, 4)}:{r.randint(-4, 6)}])",
    lambda r: f"print({_text(r)}[::{r.choice([1, 2, -1, -2])}])",
    lambda r: f"print(len({_text(r)}), {_text(r)}.upper(), {_text(r)}.find({_text(r)}))",
    lambda r: f"print({_text(r)} + {_text(r)}, {_text(r)} * {r.randint(-1, 3)})",
    lambda r: f"print(str({_float(r)}), str({_text(r)}), str({_int(r)}))",
    lambda r: f"print(int({_float(r)}), float({_int(r)}), abs({_int(r)}))",
    lambda r: f"xs = {_ints(r, 0, 5)}\n    print(xs[{r.randint(-3, 4)}])",
    lambda r: f"xs = {_ints(r, 0, 6)}\n    print(sorted(xs), xs{_slice(r)})",
    lambda r: f"xs = {_ints(r, 1, 6)}\n    print(min(xs), max(xs), sum(xs))",
    lambda r: f"xs = {_floats(r, 1, 5)}\n    print(sorted(xs), sum(xs))",
    lambda r: f'd = {{"a": {_int(r)}, "b": {_int(r)}}}\n    print(d[{_text(r)}], len(d))',
    lambda r: f'd = {{"a": {_int(r)}}}\n    d[{_text(r)}] = {_int(r)}\n    print(d, list(d))',
    lambda r: f"print(divmod({_int(r)}, {_int(r)}), {_rounds(r)})",
]
"""Each a statement or two printing what it computes, over typed values: integers to the ends of
`int`, floats from negative zero to 1e300, text with and without non-ASCII letters, lists and dicts;
operators, slices, methods and format specs that can fail, so errors are generated as often as
values."""


def case(rng: random.Random) -> str:
    """One generated case: the body of a function printing what it computes."""
    return rng.choice(TEMPLATES)(rng)


def program(cases: list[str]) -> str:
    """The cases as one program, each a function called from `main` in order."""
    parts = [f"fn case_{k}():\n    {body}\n" for k, body in enumerate(cases)]
    calls = "".join(f"    case_{k}()\n" for k in range(len(cases)))
    return "\n".join(parts) + "\nfn main():\n" + (calls or "    pass\n")


def lines_of(cases: list[str]) -> list[range]:
    """The lines, from 1, each case's function takes in `program(cases)`."""
    spans, line = [], 1
    for body in cases:
        size = 1 + body.count("\n") + 1
        spans.append(range(line, line + size))
        line += size + 1
    return spans


ERROR_LINE = re.compile(r"^case\.lot:(\d+):\d+: error", re.MULTILINE)


def checked(cases: list[str]) -> list[str]:
    """The cases the checker takes, as one program: a template can write what lotml refuses, a
    shift by a negative count say, and a program refused is a batch wasted on both targets."""
    while cases:
        with tempfile.TemporaryDirectory(prefix="lotml-differential-") as directory:
            root = Path(directory)
            (root / "case.lot").write_text(program(cases), encoding="utf-8")
            said = compiler(["check", "case.lot"], root, TIMEOUT)
        lines = {int(m) for m in ERROR_LINE.findall(said.stdout)}
        if said.returncode == 0 or not lines:
            return cases
        spans = lines_of(cases)
        kept = [c for c, span in zip(cases, spans, strict=True) if not lines.intersection(span)]
        if len(kept) == len(cases):
            return []
        cases = kept
    return cases


def native_environment() -> dict[str, str]:
    """The parity suite's environment for native builds, with the program built under
    AddressSanitizer and UndefinedBehaviorSanitizer and its cells counted (task 2.3)."""
    env = parity.native_environment() | {
        "LOTML_SANITIZE": "1",
        "LOTML_COUNT_CELLS": "1",
        "ASAN_OPTIONS": "detect_leaks=0",
    }
    runtime = sanitizer_path()
    if runtime is not None:
        key = next((k for k in env if k.upper() == "PATH"), "PATH")
        env[key] = runtime + os.pathsep + env.get(key, "")
    return env


def python_environment() -> dict[str, str]:
    """The harness's environment for the Python target, printing UTF-8 as the native target does:
    with its output piped, CPython on Windows would print in the ANSI code page (n-0100), a
    difference of the console, not of the program."""
    return child_environment() | {"PYTHONUTF8": "1"}


def run(text: str, target: str) -> Run:
    """`text` run with `lotml run` on `target`, in a directory of its own thrown away after."""
    with tempfile.TemporaryDirectory(prefix="lotml-differential-") as directory:
        root = Path(directory)
        (root / "case.lot").write_text(text, encoding="utf-8")
        env = native_environment() if target == "llvm" else python_environment()
        try:
            ran = compiler(["run", "--target", target, "case.lot"], root, TIMEOUT, env=env)
        except subprocess.TimeoutExpired:
            return Run("", None, "", None)
    panic = PANIC.search(ran.stdout + ran.stderr)
    live = LIVE.search(ran.stderr)
    stopped = STOPPED.findall(ran.stderr)
    return Run(
        ran.stdout.replace("\r\n", "\n"),
        ran.returncode,
        panic.group(1) if panic else "",
        int(live.group(1)) if live else None,
        int(stopped[-1]) if panic and stopped else None,
    )


def verdict(python: Run, native: Run) -> Verdict:
    """Whether the LLVM target did what the Python target did: the same output, the same ending,
    and for a panic the same kind of error; and, natively, no cell left at a normal exit."""
    if python.status is None or native.status is None:
        which = "Python" if python.status is None else "LLVM"
        return Verdict(
            python.status is None and native.status is None, f"the {which} target timed out"
        )
    if (python.stdout, python.status, python.kind) != (native.stdout, native.status, native.kind):
        if python.stdout != native.stdout:
            a, b = python.stdout.splitlines(), native.stdout.splitlines()
            line = next(
                (k for k, (x, y) in enumerate(zip(a, b, strict=False)) if x != y),
                min(len(a), len(b)),
            )
            return Verdict(
                False, f"line {line + 1}: Python {a[line : line + 1]}, LLVM {b[line : line + 1]}"
            )
        ended = f"Python ended {python.status} {python.kind}, LLVM {native.status} {native.kind}"
        return Verdict(False, " ".join(ended.split()))
    if native.status == 0 and native.live not in (None, 0):
        return Verdict(False, f"{native.live} cells live at exit")
    return Verdict(True, "")


def differs(text: str) -> Verdict:
    return verdict(run(text, "python"), run(text, "llvm"))


def minimize(text: str, still: Callable[[str], bool]) -> str:
    """`text` with chunks of lines deleted greedily, halving the chunk, while `still` holds."""
    lines = text.splitlines(keepends=True)
    chunk = max(1, len(lines) // 2)
    while chunk >= 1:
        k = 0
        while k < len(lines):
            trial = lines[:k] + lines[k + chunk :]
            if trial and still("".join(trial)):
                lines = trial
            else:
                k += chunk
        chunk //= 2
    return "".join(lines)


def sanitizer_path() -> str | None:
    """On Windows, the directory of clang's sanitizer runtime, which a program built under
    AddressSanitizer loads as a DLL and finds only on `PATH`; elsewhere it is linked in."""
    if os.name != "nt":
        return None
    named = os.environ.get("LOTML_CLANG")
    installer = (
        Path(os.environ.get("PROGRAMFILES", r"C:\Program Files")) / "LLVM" / "bin" / "clang.exe"
    )
    clang = named or shutil.which("clang") or (str(installer) if installer.is_file() else None)
    if clang is None:
        return None
    found = subprocess.run(  # noqa: S603 - the clang the native build uses
        [clang, "-print-resource-dir"], capture_output=True, text=True, check=False
    )
    directory = Path(found.stdout.strip()) / "lib" / "windows"
    return str(directory) if found.returncode == 0 and directory.is_dir() else None


def mutants(rng: random.Random, count: int) -> list[str]:
    """`count` mutants of corpus programs, from `lotml dev mutate`, each a program with its `test`
    blocks; the checker refuses most of them, and those are left out by the run."""
    rows = [json.loads(line) for line in CORPUS.read_text(encoding="utf-8").splitlines()]
    found: list[str] = []
    for row in rng.sample(rows, len(rows)):
        if len(found) >= count:
            break
        with tempfile.TemporaryDirectory(prefix="lotml-mutate-") as directory:
            root = Path(directory)
            (root / "p.lot").write_text(row["lotml"], encoding="utf-8")
            listed = compiler(["dev", "mutate", "--json", "p.lot"], root, TIMEOUT)
        try:
            texts = [m["text"] for m in json.loads(listed.stdout)]
        except (json.JSONDecodeError, KeyError, TypeError):
            continue
        found += rng.sample(texts, min(len(texts), 2))
    return found[:count]


def tested(text: str) -> parity.Outcome:
    """A program with `test` blocks run with `lotml test --json` on both targets, compared as the
    parity suite compares them."""
    with tempfile.TemporaryDirectory(prefix="lotml-differential-") as directory:
        root = Path(directory)
        (root / "case.lot").write_text(text, encoding="utf-8")
        try:
            python = compiler(["test", "--json", "case.lot"], root, TIMEOUT, python_environment())
            native = compiler(
                ["test", "--json", "--target", "llvm", "case.lot"],
                root,
                TIMEOUT,
                env=native_environment(),
            )
        except subprocess.TimeoutExpired:
            return parity.Outcome("mutant", "differs", "timed out")
    return parity.compare("mutant", by_kind(python), by_kind(native))


def by_kind(ran: subprocess.CompletedProcess) -> subprocess.CompletedProcess:
    """A `lotml test --json` report with each panic's message and trace left out, so two reports
    are compared by the kind of error that stopped a test, never by its words or by how many
    frames a target can name (the native target names the one that panicked)."""
    rows = parity.tests_of(ran)
    if rows is None:
        return ran
    kept = [
        {
            k: v
            for k, v in row.items()
            if not (row.get("outcome") == "panic" and k in ("message", "trace"))
        }
        for row in rows
    ]
    stdout = json.dumps({"tests": kept}) + "\n"
    return subprocess.CompletedProcess(ran.args, ran.returncode, stdout, ran.stderr)


def mutant_differs(text: str) -> parity.Outcome | None:
    """The outcome of a mutant that differs between the targets; `None` when it does not, or when
    the Python target gave no report, as when the checker refuses the mutant, which it does on
    both targets alike."""
    outcome = tested(text)
    if outcome.verdict == "differs":
        return outcome
    if outcome.verdict == "no report" and not outcome.detail.startswith("the Python target"):
        return outcome
    return None


@dataclass
class Finding:
    source: str
    """`generated` or `mutant`."""
    detail: str
    program: str
    """The program, minimized."""


def batch(cases: list[str]) -> tuple[int, str, Verdict] | None:
    """The cases run on both targets, until they differ: how many programs that took, and the one
    that differed with what differed; `None` when none did. Both targets stopping at the same
    case's panic is agreement, and the cases after it run as a program of their own."""
    runs = 0
    while cases:
        text = program(cases)
        python, native = run(text, "python"), run(text, "llvm")
        runs += 1
        found = verdict(python, native)
        if not found.same:
            return runs, text, found
        if python.stopped is None:
            return None
        cases = cases[python.stopped + 1 :]
    return None


def fuzz(seed: int, batches: int, mutant_count: int) -> tuple[int, list[Finding]]:
    """Programs generated from `seed` and mutants of the corpus, each on both targets: how many ran,
    and each that differed, minimized."""
    rng = random.Random(seed)  # noqa: S311 - a fuzzer's reproducible stream, not a secret
    findings: list[Finding] = []
    ran = 0
    for _ in range(batches):
        result = batch(checked([case(rng) for _ in range(CASES)]))
        ran += 1
        if result is not None:
            _, text, found = result
            small = minimize(text, lambda t: not differs(t).same)
            findings.append(Finding("generated", differs(small).detail or found.detail, small))
    for text in mutants(rng, mutant_count):
        outcome = mutant_differs(text)
        ran += 1
        if outcome is not None:
            small = minimize(text, lambda t: mutant_differs(t) is not None)
            again = mutant_differs(small)
            findings.append(Finding("mutant", again.detail if again else outcome.detail, small))
    return ran, findings


def write_finding(k: int, seed: int, finding: Finding) -> Path:
    """A finding as a parity program under `FOUND`: the program, after a comment saying what the
    two targets did."""
    FOUND.mkdir(parents=True, exist_ok=True)
    path = FOUND / f"seed{seed}-{k}.lot"
    detail = " ".join(finding.detail.split())
    header = f"# differential, {finding.source}, seed {seed}: {detail}\n"
    path.write_text(header + finding.program, encoding="utf-8", newline="\n")
    return path


def markdown(seed: int, ran: int, written: list[Path], findings: list[Finding]) -> str:
    lines = [
        "# The differential fuzzer",
        "",
        "Generated by `python -m lotml_harness.experiments.differential`. Typed programs from the",
        f"templates, {CASES} cases each, and mutants of the corpus from `lotml dev mutate` run on",
        "the Python target, the oracle, and on the LLVM target built under the sanitizers with its",
        "cells counted; the two must print the same, end the same and panic with the same kind of",
        "error, and a native program must free every cell.",
        "",
        f"Seed {seed}: {ran} programs ran, {len(findings)} differed.",
        "",
    ]
    for path, finding in zip(written, findings, strict=True):
        lines.append(
            f"- `{path.relative_to(RESULTS).as_posix()}` ({finding.source}): {finding.detail}"
        )
    if findings:
        lines.append("")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(prog="python -m lotml_harness.experiments.differential")
    parser.add_argument("--seed", type=int, default=0)
    parser.add_argument("--batches", type=int, default=20, help="generated programs to run")
    parser.add_argument("--mutants", type=int, default=20, help="mutants of the corpus to run")
    args = parser.parse_args(argv)
    ran, findings = fuzz(args.seed, args.batches, args.mutants)
    written = [write_finding(k, args.seed, f) for k, f in enumerate(findings)]
    report = markdown(args.seed, ran, written, findings)
    REPORT.parent.mkdir(parents=True, exist_ok=True)
    REPORT.write_text(report, encoding="utf-8", newline="\n")
    sys.stdout.buffer.write(report.encode("utf-8"))
    return 1 if findings else 0


if __name__ == "__main__":
    sys.exit(main())
