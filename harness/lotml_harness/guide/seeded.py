"""Seeded failures: lotml programs that check and pass their tests, mutated the ways agents break
them, each mutant kept as a repair whose known fix is the program (specs/seeded-failures/).

    python -m lotml_harness.guide.seeded

The programs come from sources that may be trained on, from train and validation problems only:
the agent benchmark's solutions, the original HumanEval's canonical solutions translated by the
corpus's rules alone, and the final files of the trajectories the trace dataset exported.
"""

import argparse
import datetime
import hashlib
import json
import random
import re
import tempfile
from collections import Counter, defaultdict
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from pathlib import Path

from lotml_harness import split
from lotml_harness.agent import dataset, humaneval, safe
from lotml_harness.agent.bench import AgentTask, tasks
from lotml_harness.agent.grade import grade
from lotml_harness.corpus import rules
from lotml_harness.experiments.phase1 import RESULTS, Lotml
from lotml_harness.tasks import Task
from lotml_harness.tasks.sources import CACHE

BENCH_NOTICE = "Copyright (c) 2026 Mateus Andrade, MIT License (LICENSE)"
"""The agent benchmark is this repository's own."""
NOTICES = {"bench": BENCH_NOTICE, "humaneval-original": humaneval.HUMANEVAL_NOTICE}
"""The notice each source a program may come from requires beside its records."""
HELD_OUT = "held-out"


class HeldOut(ValueError):
    """A program of a held-out problem: training on it would leak the guide's evaluation."""


@dataclass(frozen=True)
class Program:
    """Files that check and pass their tests, the graded ones with their hidden blocks appended,
    as the grader builds them. Mutants fall in `graded`; the blocks judge them."""

    task: str
    problem: str
    split: str
    source: str
    prompt: str
    files: dict[str, str]
    graded: tuple[str, ...]
    notice: str


def program(
    ident: str, source: str, prompt: str, files: dict[str, str], graded, notice: str
) -> Program:
    """A program of the problem `ident` derives from, refusing a held-out one (R3.2)."""
    problem = split.problem(ident)
    bucket = split.split(problem)
    if bucket == HELD_OUT:
        raise HeldOut(f"{ident} is {problem}, which the split holds out")
    return Program(ident, problem, bucket, source, prompt, files, tuple(graded), notice)


def with_hidden(task: AgentTask, files: dict[str, str]) -> dict[str, str]:
    """`files` with each graded file's hidden blocks appended, as `grade` appends them."""
    joined = dict(files)
    for name in task.graded:
        joined[name] = joined.get(name, "").rstrip("\n") + "\n\n" + task.hidden(name)
    return joined


def bench_programs(found: list[AgentTask] | None = None) -> list[Program]:
    """The agent benchmark's reference solutions, laid over their workspaces, with their hidden
    blocks: all train, since the benchmark's eight tasks are ours and too few to hold out."""
    found = tasks() if found is None else found
    return [
        program(
            task.id,
            "bench",
            task.prompt,
            with_hidden(task, task.workspace_files | task.solution_files),
            task.graded,
            BENCH_NOTICE,
        )
        for task in found
    ]


@dataclass
class Taken:
    """What a source gave: its programs, and the problems left out by reason."""

    programs: list[Program] = field(default_factory=list)
    left_out: Counter = field(default_factory=Counter)


def translated(record: dict, lotml: Lotml | None = None) -> tuple[str, str]:
    """A HumanEval problem's canonical solution in lotml with its hidden blocks appended, typed
    from its recorded cases as its agent task's hidden blocks are; `Refused` with the reason when
    the rules cannot write it or what they write does not check and pass those blocks."""
    entry = record["entry_point"]
    names, _ = humaneval._function(record["prompt"], entry)
    params, returns, blocks = humaneval.typed(
        entry, names, humaneval.record_humaneval(record), lotml
    )
    task = Task(
        id=str(record["task_id"]),
        source="humaneval-original",
        name=entry,
        params=list(zip(names, params, strict=True)),
        returns=returns,
        doc="",
        tests=[],
        canonical=record["prompt"] + record["canonical_solution"],
    )
    translation = rules.translate(task)
    if translation.code is None:
        raise humaneval.Refused("rules", "; ".join(translation.reasons))
    graded = AgentTask(
        id=task.id,
        kind="implement",
        graded=("solution.lotml",),
        prompt="",
        workspace_files={"solution.lotml": translation.code},
        hidden_files={"solution.lotml": blocks},
    )
    with tempfile.TemporaryDirectory(prefix="lotml-seeded-", ignore_cleanup_errors=True) as scratch:
        graded.lay(Path(scratch))
        result = grade(graded, Path(scratch), lotml)
    if result.outcome != "pass":
        raise humaneval.Refused(result.outcome.replace(" ", "-"), "; ".join(result.failures[:3]))
    return translation.code, blocks


def humaneval_programs(lotml: Lotml | None = None, workers: int = 8) -> Taken:
    """The original HumanEval's train and validation problems whose canonical solutions the
    rules translate into a program that checks and passes its hidden blocks (R1.2, R1.3)."""
    records, _ = humaneval.read_humaneval()
    wanted = []
    for record in records:
        ident = humaneval._ident(record)
        if split.split(split.problem(ident)) != HELD_OUT:
            wanted.append((ident, record))

    def one(job: tuple[str, dict]) -> Program | str:
        ident, record = job
        try:
            code, blocks = translated(record, lotml)
        except humaneval.Refused as refusal:
            return refusal.reason
        prompt = humaneval.PROMPT.format(name=record["entry_point"])
        files = {"solution.lotml": code.rstrip("\n") + "\n\n" + blocks}
        return program(
            ident,
            "humaneval-original",
            prompt,
            files,
            ("solution.lotml",),
            humaneval.HUMANEVAL_NOTICE,
        )

    taken = Taken()
    with ThreadPoolExecutor(max_workers=workers) as pool:
        for outcome in pool.map(one, wanted):
            if isinstance(outcome, Program):
                taken.programs.append(outcome)
            else:
                taken.left_out[outcome] += 1
    return taken


def trajectory_programs(export: Path | None, hidden_for) -> Taken:
    """The files at the end of the passing trajectories the trace dataset exported to `export`,
    from its `train/` and `validation/` output only, each with its task's hidden blocks from
    `hidden_for(task_id)`, an `AgentTask`. No export yet gives nothing."""
    taken = Taken()
    if export is None:
        return taken
    for bucket in ("train", "validation"):
        path = export / bucket / "trajectories.jsonl"
        if not path.is_file():
            continue
        for line in path.read_text(encoding="utf-8").splitlines():
            if not line.strip():
                continue
            record = json.loads(line)
            meta, files = record.get("meta", {}), record.get("files")
            if not isinstance(files, dict) or "task" not in meta or "source" not in meta:
                taken.left_out["no files or task"] += 1
                continue
            if meta["source"] not in NOTICES:
                taken.left_out["a source with no notice"] += 1
                continue
            try:
                problem = split.problem(meta["task"])
            except ValueError:
                taken.left_out["an unknown task"] += 1
                continue
            if split.split(problem) == HELD_OUT:
                raise HeldOut(f"{meta['task']} is {problem}, which the split holds out")
            try:
                for name, text in files.items():
                    safe.checked_name(name)
                    if not isinstance(text, str):
                        raise ValueError(f"{name!r} holds no text")
                task = hidden_for(meta["task"])
                joined = with_hidden(task, files)
            except (ValueError, KeyError, StopIteration) as refusal:
                taken.left_out[f"unusable files: {type(refusal).__name__}"] += 1
                continue
            taken.programs.append(
                program(
                    meta["task"],
                    meta["source"],
                    task.prompt,
                    joined,
                    task.graded,
                    NOTICES[meta["source"]],
                )
            )
    return taken


FAMILIES = ("names", "types", "calls", "mutability", "meaning")
CODES = {
    "E0201": "names",
    "E0205": "names",
    "E0212": "names",
    "E0202": "types",
    "E0204": "types",
    "E0207": "types",
    "E0203": "calls",
    "E0214": "calls",
    "E0219": "calls",
    "E0301": "mutability",
    "E0302": "mutability",
    "E0303": "mutability",
}
"""The family of operators that aims at each diagnostic (specs/seeded-failures/ design)."""
BUDGET = 60
"""Mutants drawn per program."""
PHASE1 = RESULTS / "phase1.md"
SEED = "lotml-seeded-1"
CODE_COUNT = re.compile(r"(E\d{4}) (\d+)")


@dataclass
class Weights:
    """How often agents' failures fall in each family, and what the counts came from."""

    families: Counter
    unmapped: Counter
    sources: list[str]


def weights(phase1: str, repairs: Counter | None = None) -> Weights:
    """Family weights from the phase 1 gate's committed report — the first error code of every
    lotml answer the compiler refused, and the answers whose tests failed for `meaning` — plus the
    codes of real repairs once the trace dataset has them. Aggregates only: no prompt or answer of
    the gate enters the data (harness/results/NOTICE.md)."""
    families: Counter = Counter(dict.fromkeys(FAMILIES, 0))
    unmapped: Counter = Counter()
    for line in phase1.splitlines():
        cells = [c.strip() for c in line.strip().strip("|").split("|")]
        if len(cells) != 7 or cells[1] != "lotml" or not cells[4].isdigit():
            continue
        families["meaning"] += int(cells[4])
        for code, count in CODE_COUNT.findall(cells[6]):
            if code in CODES:
                families[CODES[code]] += int(count)
            else:
                unmapped[code] += int(count)
    sources = ["harness/results/phase1.md"]
    for code, count in (repairs or Counter()).items():
        if code in CODES:
            families[CODES[code]] += count
        else:
            unmapped[code] += count
    if repairs:
        sources.append("the trace dataset's repairs")
    return Weights(families, unmapped, sources)


def apportion(budget: int, weights: dict[str, int]) -> dict[str, int]:
    """`budget` split by largest remainder in proportion to `weights`; ties go to the heavier
    family, then to the earlier one in `FAMILIES`."""
    total = sum(weights.values())
    if total == 0 or budget == 0:
        return dict.fromkeys(weights, 0)
    quotas = {f: budget * w / total for f, w in weights.items()}
    shares = {f: int(q) for f, q in quotas.items()}
    order = sorted(
        weights, key=lambda f: (-(quotas[f] - shares[f]), -weights[f], FAMILIES.index(f))
    )
    for family in order[: budget - sum(shares.values())]:
        shares[family] += 1
    return shares


def shares(budget: int, weights: dict[str, int], available: dict[str, int]) -> dict[str, int]:
    """Each family's part of one program's budget: in proportion to its weight, and a family with
    fewer mutants than its part passes the rest to the others, again by weight (R2.5)."""
    given = dict.fromkeys(FAMILIES, 0)
    open_ = [f for f in FAMILIES if weights.get(f, 0) > 0 and available.get(f, 0) > 0]
    left = budget
    while left > 0 and open_:
        parts = apportion(left, {f: weights[f] for f in open_})
        left = 0
        for family in open_:
            take = min(parts[family], available[family] - given[family])
            given[family] += take
            left += parts[family] - take
        open_ = [f for f in open_ if given[f] < available[f]]
    return given


def mutant_key(problem: str, text: str) -> str:
    return problem + ":" + hashlib.sha256(text.encode("utf-8")).hexdigest()


def draw(
    program: Program, listed: list[dict], weights: dict[str, int], seen: set[str]
) -> list[dict]:
    """The program's mutants to judge: its budget shared among the families, drawn uniformly
    within each by a generator seeded with the program, each identical mutant once per problem
    — `seen` holds the problem's keys already drawn (R2.5, R2.6)."""
    pools: dict[str, list[dict]] = {f: [] for f in FAMILIES}
    for mutant in listed:
        key = mutant_key(program.problem, mutant["text"])
        if key in seen or mutant["family"] not in pools:
            continue
        seen.add(key)
        pools[mutant["family"]].append(mutant)
    parts = shares(BUDGET, weights, {f: len(pool) for f, pool in pools.items()})
    generator = random.Random(f"{SEED}:{program.task}")  # noqa: S311 - a reproducible draw
    drawn = []
    for family in FAMILIES:
        drawn += generator.sample(pools[family], parts[family])
    return drawn


@dataclass
class Verdict:
    """What judging one mutant found: kept as a failure `check` refuses or a test then fails, or
    dropped — equivalent or untested, fixed by `check --fix`, or out of time."""

    kept: bool
    reason: str
    diagnostics: list[dict] | None = None
    failing: dict | None = None


def _json(text: str, key: str) -> dict | None:
    """The last line of `text` that is a JSON object holding `key`."""
    for line in reversed(text.strip().splitlines()):
        try:
            found = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(found, dict) and key in found:
            return found
    return None


def listed(program: Program, deadline: float = safe.DEADLINE) -> tuple[list[dict], Counter]:
    """Every mutant `lotml dev mutate` lists in the program's graded files, each with its file;
    and, by reason, the files it could not list."""
    found, failed = [], Counter()
    with tempfile.TemporaryDirectory(prefix="lotml-seeded-", ignore_cleanup_errors=True) as scratch:
        try:
            safe.lay(Path(scratch), program.files)
        except ValueError:
            failed["unsafe files"] += len(program.graded)
            return found, failed
        for name in program.graded:
            ran = safe.lotml(["dev", "mutate", "--json"], [name], Path(scratch), deadline)
            if ran is None:
                failed["mutate timed out"] += 1
                continue
            try:
                listing = json.loads(ran.stdout) if ran.returncode == 0 else None
            except json.JSONDecodeError:
                listing = None
            if not isinstance(listing, list):
                failed["mutate failed"] += 1
                continue
            found += [mutant | {"file": name} for mutant in listing]
    return found, failed


def judge(program: Program, file: str, text: str, deadline: float = safe.DEADLINE) -> Verdict:
    """One mutant judged in a scratch copy laid by the safe layer: kept when `check` refuses it
    and `check --fix` does not make it clean, or when it checks and one of the program's tests
    then fails (R2.3, R2.4, R2.7)."""
    with tempfile.TemporaryDirectory(prefix="lotml-seeded-", ignore_cleanup_errors=True) as scratch:
        root = Path(scratch)
        try:
            names = safe.lay(root, program.files | {file: text})
        except ValueError:
            return Verdict(False, "refused")
        checked = safe.lotml(["check", "--json"], names, root, deadline)
        if checked is None:
            return Verdict(False, "timeout")
        report = _json(checked.stdout, "diagnostics")
        if report is None:
            return Verdict(False, "no report")
        errors = [d for d in report["diagnostics"] if d.get("severity") == "error"]
        if errors:
            fixed = safe.lotml(["check", "--fix", "--json"], names, root, deadline)
            if fixed is None:
                return Verdict(False, "timeout")
            after = _json(fixed.stdout, "summary")
            if after is None:
                return Verdict(False, "no report")
            if after["summary"].get("errors") == 0:
                return Verdict(False, "fixable")
            return Verdict(True, "check", diagnostics=errors)
        tested = safe.lotml(["test", "--json"], [file], root, deadline)
        if tested is None:
            return Verdict(False, "timeout")
        report = _json(tested.stdout, "tests")
        if report is None:
            return Verdict(False, "no report")
        failing = next((test for test in report["tests"] if test.get("outcome") != "pass"), None)
        if failing is None:
            return Verdict(False, "equivalent")
        return Verdict(True, "test", failing=failing)


@dataclass
class Tally:
    """Every count the committed report gives."""

    programs: Counter = field(default_factory=Counter)
    left_out: dict[str, Counter] = field(default_factory=dict)
    unlisted: Counter = field(default_factory=Counter)
    splits: Counter = field(default_factory=Counter)
    listed: Counter = field(default_factory=Counter)
    drawn: Counter = field(default_factory=Counter)
    outcomes: dict[tuple[str, str], Counter] = field(default_factory=lambda: defaultdict(Counter))


def seed(
    programs: list[Program], weighting: Weights, compiler: str, workers: int = 8
) -> tuple[list[dict], Tally]:
    """Every program's mutants listed, drawn and judged; the kept ones as repair records marked
    `origin: seeded` (R3.1), and the tally of all of it."""
    tally = Tally()
    seen: dict[str, set[str]] = defaultdict(set)
    jobs = []
    for program in programs:
        tally.programs[program.source] += 1
        found, failed = listed(program)
        tally.unlisted.update(failed)
        for mutant in found:
            tally.listed[(mutant["family"], mutant["operator"])] += 1
        for mutant in draw(program, found, dict(weighting.families), seen[program.problem]):
            tally.drawn[(mutant["family"], mutant["operator"])] += 1
            jobs.append((program, mutant))

    def one(job: tuple[Program, dict]) -> Verdict:
        program, mutant = job
        return judge(program, mutant["file"], mutant["text"])

    with ThreadPoolExecutor(max_workers=workers) as pool:
        verdicts = list(pool.map(one, jobs))
    records = []
    for (program, mutant), verdict in zip(jobs, verdicts, strict=True):
        tally.outcomes[(mutant["family"], mutant["operator"])][verdict.reason] += 1
        if not verdict.kept:
            continue
        tally.splits[program.split] += 1
        meta = {
            "task": program.task,
            "source": program.source,
            "model": None,
            "arm": None,
            "compiler": compiler,
            "problem": program.problem,
            "split": program.split,
            "origin": "seeded",
            "operator": mutant["operator"],
            "family": mutant["family"],
            "declaration": mutant["declaration"],
        }
        records.append(
            dataset.repair(
                program.prompt,
                mutant["file"],
                mutant["text"],
                program.files[mutant["file"]],
                meta,
                diagnostics=verdict.diagnostics,
                failing=verdict.failing,
            )
        )
    return records, tally


SEEDED = CACHE / "guide" / "seeded"
REPORT = RESULTS / "seeded.md"
REASONS = ("check", "test", "equivalent", "fixable", "timeout", "no report", "refused")


def write(records: list[dict], programs: list[Program], day: str, root: Path = SEEDED) -> Path:
    """The records to the git-ignored cache, train and validation apart, with the notices of the
    sources they came from beside them (R3.3). A held-out record stops the write."""
    out = root / day
    by_split: dict[str, list[dict]] = {"train": [], "validation": []}
    for record in records:
        bucket = record["meta"]["split"]
        if bucket not in by_split:
            raise HeldOut(f"{record['meta']['problem']} is {bucket}")
        by_split[bucket].append(record)
    for bucket, kept in by_split.items():
        (out / bucket).mkdir(parents=True, exist_ok=True)
        lines = "".join(json.dumps(r, ensure_ascii=False) + "\n" for r in kept)
        (out / bucket / "repairs.jsonl").write_text(lines, encoding="utf-8")
    used = {r["meta"]["source"] for r in records}
    notices = {p.source: p.notice for p in programs if p.source in used}
    text = "".join(f"## {source}\n\n{notices[source].rstrip()}\n\n" for source in sorted(notices))
    (out / "NOTICE").write_text(text, encoding="utf-8")
    return out


def markdown(tally: Tally, weighting: Weights, compiler: str, day: str) -> str:
    """The committed report: programs by source and what was left out, the counts that weighted
    the draw, and the mutants made, kept and dropped by operator and reason (R3.3)."""
    parts = apportion(BUDGET, dict(weighting.families))
    lines = [
        "# Seeded failures",
        "",
        f"Written by `python -m lotml_harness.guide.seeded` on {day} with `{compiler}`; the",
        f"records are in `harness/cache/guide/seeded/{day}/`, git-ignored.",
        "",
        "## Programs",
        "",
        "| source | programs | left out |",
        "|---|---:|---|",
    ]
    for source in sorted(set(tally.programs) | set(tally.left_out)):
        out = tally.left_out.get(source, Counter())
        why = ", ".join(f"{r} {n}" for r, n in sorted(out.items())) or "—"
        lines.append(f"| {source} | {tally.programs[source]} | {why} |")
    if tally.unlisted:
        unlisted = ", ".join(f"{r} {n}" for r, n in sorted(tally.unlisted.items()))
        lines += ["", f"Graded files whose mutants could not be listed: {unlisted}."]
    lines += [
        "",
        "| split | records |",
        "|---|---:|",
        *(f"| {s} | {tally.splits[s]} |" for s in ("train", "validation")),
        "",
        "## Weights",
        "",
        f"From {', '.join(weighting.sources)}: each family's count, and its part of a",
        f"program's {BUDGET} mutants before a short family passes its share on.",
        "",
        "| family | count | per program |",
        "|---|---:|---:|",
        *(f"| {f} | {weighting.families[f]} | {parts[f]} |" for f in FAMILIES),
        "",
        "Codes no operator aims at: "
        + (", ".join(f"{c} {n}" for c, n in sorted(weighting.unmapped.items())) or "none")
        + ".",
        "",
        "## Mutants by operator",
        "",
        "| family | operator | listed | drawn | " + " | ".join(REASONS) + " |",
        "|---|---|" + "---:|" * (2 + len(REASONS)),
    ]
    for key in sorted(tally.listed, key=lambda k: (FAMILIES.index(k[0]), k[1])):
        outcome = tally.outcomes.get(key, Counter())
        counts = " | ".join(str(outcome[r]) for r in REASONS)
        lines.append(
            f"| {key[0]} | {key[1]} | {tally.listed[key]} | {tally.drawn[key]} | {counts} |"
        )
    totals = Counter()
    for outcome in tally.outcomes.values():
        totals.update(outcome)
    lines += [
        "",
        f"Kept {totals['check'] + totals['test']} of {sum(tally.drawn.values())} drawn: "
        f"{totals['check']} refused by `check`, {totals['test']} failing a test.",
        "",
    ]
    return "\n".join(lines)


def latest_export(root: Path = CACHE / "dataset") -> Path | None:
    """The trace dataset's newest export, or None before the first."""
    found = sorted(p for p in root.glob("*") if p.is_dir()) if root.is_dir() else []
    return found[-1] if found else None


def repair_codes(export: Path | None) -> Counter:
    """The first diagnostic code of each real repair in an export, for the weights."""
    codes: Counter = Counter()
    if export is None:
        return codes
    for bucket in ("train", "validation"):
        path = export / bucket / "repairs.jsonl"
        if not path.is_file():
            continue
        for line in path.read_text(encoding="utf-8").splitlines():
            diagnostics = json.loads(line).get("diagnostics") or []
            if diagnostics:
                codes[diagnostics[0].get("code", "")] += 1
    return codes


def hidden_for(task_id: str) -> AgentTask:
    """A trajectory's task, to append its hidden blocks: the benchmark's or HumanEval's."""
    if task_id.startswith("humaneval-"):
        records, _ = humaneval.read_humaneval()
        number = int(task_id.removeprefix("humaneval-"))
        record = next(r for r in records if r["task_id"] == f"HumanEval/{number}")
        return humaneval.pose_humaneval(record)
    return next(t for t in tasks() if t.id == task_id)


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--workers", type=int, default=8)
    parser.add_argument("--limit", type=int, help="only the first N programs, for a trial")
    args = parser.parse_args(argv)
    export = latest_export()
    taken_humaneval = humaneval_programs(workers=args.workers)
    taken_trajectories = trajectory_programs(export, hidden_for)
    programs = bench_programs() + taken_humaneval.programs + taken_trajectories.programs
    programs = programs[: args.limit] if args.limit else programs
    weighting = weights(PHASE1.read_text(encoding="utf-8"), repair_codes(export))
    version = Lotml().compiler(["--version"], ".")
    compiler = version.stdout.strip() if version is not None else "unknown"
    records, tally = seed(programs, weighting, compiler, args.workers)
    tally.left_out["humaneval-original"] = taken_humaneval.left_out
    if taken_trajectories.left_out:
        tally.left_out["trajectories"] = taken_trajectories.left_out
    day = datetime.date.today().isoformat()
    out = write(records, programs, day)
    text = markdown(tally, weighting, compiler, day)
    REPORT.write_text(text, encoding="utf-8")
    print(text)
    print(f"{len(records)} records in {out}")


if __name__ == "__main__":
    main()
