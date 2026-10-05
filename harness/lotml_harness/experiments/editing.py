"""The indented form against a braces form, on editing tasks at scale.

Each task is a long file of working lotml functions — the models' own passing answers from
the variant experiment — in which one function was broken by a slip: a statement moved one
block level out or in, so the file still parses and a hidden test fails. The model is told
which call fails and answers with SEARCH/REPLACE blocks, the format agents' edit tools use;
the harness applies them, parses, runs the hidden tests and reports back, for up to three
turns. The same task is given in the indented form and in the braces form, where a block is
`{ … }` and indentation means nothing.

Each answer is applied strictly (the search text byte for byte) and, as a counterfactual,
tolerantly (up to one uniform indentation offset, as aider does). Reported: tasks fixed at
the first turn and within three, how edits failed, slips the parser caught, rewrites that
quote most of the file, and C-family idioms in the edits.

    python -m lotml_harness.experiments.editing --model claude:haiku --model ollama:qwen2.5-coder:7b
"""

import argparse
import json
import random
import re
from collections import Counter
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from pathlib import Path

from lark import Token
from lark.exceptions import LarkError

from lotml_harness import ROOT, reference
from lotml_harness.execute import Result, isolated
from lotml_harness.experiments.models import Model, ModelError, from_spec
from lotml_harness.experiments.variants import RUNS as ANSWERS
from lotml_harness.experiments.variants import SAMPLE, mcnemar, percent, sample
from lotml_harness.experiments.variants import run as answer
from lotml_harness.lang.braces import from_braces, indent, line_slips, to_braces
from lotml_harness.lang.grammar import parser
from lotml_harness.tasks import Task, build, values

RESULTS = ROOT / "harness" / "results"
RUNS = RESULTS / "editing"
REPORT = RESULTS / "editing.md"
FORMS = ("indented", "braces")
TURNS = 3
FILE_LINES = 150
"""A task's file holds at least this many non-blank lines: the target and other functions."""
SEARCH, DIVIDER, REPLACE = "<<<<<<< SEARCH", "=======", ">>>>>>> REPLACE"
WRITERS = ("claude-sonnet", "claude-haiku")
"""Whose passing answers become the files, in order of preference."""


# Tasks -------------------------------------------------------------------------------


@dataclass
class EditTask:
    task: Task
    original: str
    broken: str
    slip: tuple[int, int]
    """The line moved and the direction, in the broken file."""
    failure: str


def solutions(tasks: dict[str, Task], answers: Path = ANSWERS) -> dict[str, str]:
    """A passing variant B program per task, from every stored answer of a preferred writer."""
    found: dict[str, dict[str, str]] = {}
    for path in sorted(answers.glob("*.jsonl")):
        for line in path.read_text(encoding="utf-8").splitlines():
            row = json.loads(line)
            if (
                row.get("model") in WRITERS
                and row.get("variant") == "b"
                and row.get("passed")
                and not row.get("violations")
                and row["task"] in tasks
            ):
                found.setdefault(row["task"], {}).setdefault(row["model"], row["code"])
    return {
        task: next(codes[w] for w in WRITERS if w in codes) for task, codes in sorted(found.items())
    }


def more_solutions(spec: str, count: int, seed: int = 0, workers: int = 6) -> None:
    """Variant B answers to tasks outside the variant sample, so editing has enough files."""
    pool = build.load(only=("humaneval", "mbpp"))
    sampled = {t.id for t in sample(pool, SAMPLE, seed)}
    rest = sorted((t for t in pool if t.id not in sampled), key=lambda t: t.id)
    extra = random.Random(f"{seed}/extra").sample(rest, min(count, len(rest)))  # noqa: S311
    model = from_spec(spec)
    answer(model, extra, ANSWERS / f"extra-{model.name}.jsonl", workers, variants=("b",))


def top_level_names(source: str) -> set[str]:
    tree = parser("b").parse(source if source.endswith("\n") else source + "\n")
    names = set()
    for item in tree.children:
        first = getattr(item, "children", [None])[0]
        if isinstance(first, Token) and first.type == "NAME":
            names.add(first.value)
        elif first is not None and getattr(first, "data", None) == "fn_head":
            names.add(first.children[0].value)
    return names


def call(task: Task, args: list) -> str:
    rendered = ", ".join(
        values.render(a, t, "b") for a, (_, t) in zip(args, task.params, strict=True)
    )
    return f"{task.name}({rendered})"


def first_failure(task: Task, result: Result) -> str:
    """What a failing run says: a parse error, a crash, or the first wrong case."""
    if result.error is not None:
        return f"the file does not run: {result.error}"
    for case, outcome in zip(task.tests, result.cases, strict=False):
        if outcome == "wrong answer":
            expected = values.render(case.expected, task.returns, "b")
            return f"`{call(task, case.args)}` should return `{expected}`, but it does not."
        if outcome != "pass":
            return f"`{call(task, case.args)}` stops with {outcome}."
    return "every test passes."


def slipped(task: Task, program: str, seed: str) -> tuple[str, tuple[int, int], str] | None:
    """A slip inside the task's function that still parses and fails a hidden test."""
    start, end = function_lines(program, task.name)
    candidates = [
        (row, delta, mutant) for row, delta, mutant in line_slips(program) if start < row <= end
    ]
    random.Random(seed).shuffle(candidates)  # noqa: S311
    for row, delta, mutant in candidates:
        try:
            parser("b").parse(mutant)
        except LarkError:
            continue
        result = isolated(mutant, "b", "lotml", task)
        if not result.passed:
            return mutant, (row, delta), first_failure(task, result)
    return None


def function_lines(program: str, name: str) -> tuple[int, int]:
    """The first and last line of the top-level function `name`."""
    lines = program.split("\n")
    start = next(i for i, line in enumerate(lines) if re.match(rf"fn {re.escape(name)}\b", line))
    end = start
    for index in range(start + 1, len(lines)):
        if lines[index].strip() and indent(lines[index]) == 0:
            break
        if lines[index].strip():
            end = index
    return start, end


def assemble(target: str, others: list[str], seed: str) -> tuple[str, int]:
    """The target among other programs whose names do not clash, long enough; and its index."""
    chosen, names = [target], top_level_names(target)
    for other in others:
        if sum(1 for p in chosen for line in p.split("\n") if line.strip()) >= FILE_LINES:
            break
        other_names = top_level_names(other)
        if other_names & names:
            continue
        chosen.append(other)
        names |= other_names
    order = list(range(len(chosen)))
    random.Random(seed).shuffle(order)  # noqa: S311
    programs = [chosen[i].strip("\n") for i in order]
    return "\n\n\n".join(programs) + "\n", order.index(0)


def make_tasks(tasks: dict[str, Task], programs: dict[str, str], seed: int = 0) -> list[EditTask]:
    """One editing task per solved task whose function a slip can silently break."""
    made = []
    ids = sorted(programs)
    for task_id in ids:
        task = tasks[task_id]
        others = [programs[i] for i in ids if i != task_id]
        random.Random(f"{seed}/{task_id}/others").shuffle(others)  # noqa: S311
        try:
            whole, _ = assemble(programs[task_id], others, f"{seed}/{task_id}/order")
            if not isolated(whole, "b", "lotml", task).passed:
                continue
            slip = slipped(task, whole, f"{seed}/{task_id}/slip")
        except (LarkError, StopIteration):
            continue
        if slip is not None and same_program(from_braces(to_braces(slip[0])), slip[0]):
            broken, where, failure = slip
            made.append(EditTask(task, whole, broken, where, failure))
    return made


def same_program(first: str | None, second: str) -> bool:
    """Whether two texts parse to the same program: the braces form must mean the indented."""
    if first is None:
        return False
    try:
        return parser("b").parse(first + "\n") == parser("b").parse(second + "\n")
    except LarkError:
        return False


# The forms ---------------------------------------------------------------------------

BRACES_PROSE = [
    (
        "Blocks are indented by 4 spaces after a line ending in `:`;",
        "A block is enclosed in `{` and `}`: its header line ends with `{`, and a line "
        "holding only `}` closes it; `elif` and `else` follow the `}` on its line. "
        "Indentation is not significant;",
    ),
    ("- Methods live in `impl Type:`.", "- Methods live in `impl Type { … }`."),
    ("`impl Trait for Type:` implements them.", "`impl Trait for Type { … }` implements them."),
    ('- `test "name":` blocks sit', '- `test "name" { … }` blocks sit'),
]
FENCE = re.compile(r"```\n(.*?)```", re.DOTALL)


def reference_text(form: str) -> str:
    text = reference.text()
    if form == "indented":
        return text
    for old, new in BRACES_PROSE:
        if old not in text:
            raise ValueError(f"the reference no longer says: {old!r}")
        text = text.replace(old, new)
    return FENCE.sub(lambda m: "```\n" + to_braces(m.group(1)) + "```", text)


def in_form(source: str, form: str) -> str:
    return to_braces(source) if form == "braces" else source


def to_program(text: str, form: str) -> str | None:
    """The program a file in `form` means, or None when its braces do not balance."""
    return from_braces(text) if form == "braces" else text


# Edits -------------------------------------------------------------------------------


def parse_blocks(text: str) -> list[tuple[str, str]]:
    """The SEARCH/REPLACE blocks in an answer; each side keeps its final newline."""
    blocks, state, search, replace = [], None, [], []
    for line in text.splitlines():
        marker = line.strip()
        if marker == SEARCH:
            state, search, replace = "search", [], []
        elif marker == DIVIDER and state == "search":
            state = "replace"
        elif marker == REPLACE and state == "replace":
            blocks.append(("".join(s + "\n" for s in search), "".join(r + "\n" for r in replace)))
            state = None
        elif state == "search":
            search.append(line)
        elif state == "replace":
            replace.append(line)
    return blocks


def line_starts(text: str, search: str) -> list[int]:
    found, at = [], text.find(search)
    while at >= 0:
        if at == 0 or text[at - 1] == "\n":
            found.append(at)
        at = text.find(search, at + 1)
    return found


def occurs(haystack: list[str], needle: list[str]) -> list[int]:
    return [
        i for i in range(len(haystack) - len(needle) + 1) if haystack[i : i + len(needle)] == needle
    ]


def shifted(line: str, offset: int) -> str:
    return line if not line.strip() else " " * max(0, indent(line) + offset) + line.lstrip()


def relative_match(lines: list[str], search: list[str]) -> list[tuple[int, int]]:
    """Starts where `search` matches with every line moved by one common offset."""
    anchor = next(j for j, line in enumerate(search) if line.strip())
    found = []
    for start in occurs([x.strip() for x in lines], [s.strip() for s in search]):
        offset = indent(lines[start + anchor]) - indent(search[anchor])
        if all(
            shifted(s, offset) == lines[start + j] or not s.strip() for j, s in enumerate(search)
        ):
            found.append((start, offset))
    return found


def apply(text: str, blocks, tolerant: bool = False) -> tuple[str | None, str | None]:
    """Apply `blocks` in order; the result, or None and why the edit failed."""
    if not blocks:
        return None, "no edit"
    for search, replace in blocks:
        if not search.strip():
            return None, "empty search"
        starts = line_starts(text, search)
        if len(starts) > 1:
            return None, "ambiguous"
        if len(starts) == 1:
            text = text[: starts[0]] + replace + text[starts[0] + len(search) :]
            continue
        lines, wanted = text.split("\n"), search.rstrip("\n").split("\n")
        matches = relative_match(lines, wanted) if tolerant else []
        if len(matches) == 1:
            start, offset = matches[0]
            new = [shifted(r, offset) for r in replace.rstrip("\n").split("\n")] if replace else []
            text = "\n".join(lines[:start] + new + lines[start + len(wanted) :])
            continue
        if len(matches) > 1:
            return None, "ambiguous"
        stripped = occurs([x.strip() for x in lines], [w.strip() for w in wanted])
        return None, "not found: whitespace" if stripped else "not found"
    return text, None


C_IDIOMS = {
    "else if": re.compile(r"\belse\s+if\b"),
    "&&": re.compile(r"&&"),
    "||": re.compile(r"\|\|"),
    "! not": re.compile(r"(?<![!=<>])!(?!=)\s*[A-Za-z_(]"),
    "semicolon": re.compile(r";\s*$", re.MULTILINE),
    "brace block": re.compile(r"\)\s*\{\s*$|^\s*\}\s*$", re.MULTILINE),
}


def idioms(blocks: list[tuple[str, str]], form: str) -> list[str]:
    """C-family constructs an answer's replacements introduce."""
    text = "".join(replace for _, replace in blocks)
    found = [name for name, pattern in C_IDIOMS.items() if pattern.search(text)]
    return [name for name in found if not (form == "braces" and name == "brace block")]


@dataclass
class Attempt:
    outcome: str
    edited: str | None
    feedback: str
    details: dict = field(default_factory=dict)


def judge(task: EditTask, form: str, current: str, answer: str, tolerant: bool) -> Attempt:
    """Apply an answer to the current file and say what happened."""
    blocks = parse_blocks(answer)
    edited, reason = apply(current, blocks, tolerant)
    lines = sum(1 for line in current.split("\n") if line.strip())
    quoted = sum(1 for search, _ in blocks for line in search.split("\n") if line.strip())
    details = {
        "blocks": len(blocks),
        "searched": round(quoted / lines, 3) if lines else 0,
        "idioms": idioms(blocks, form),
    }
    if edited is None:
        message = {
            "no edit": "Your answer has no SEARCH/REPLACE block.",
            "empty search": "A SEARCH section is empty.",
            "ambiguous": "A SEARCH section matches more than one place in the file.",
        }.get(reason, "A SEARCH section does not match the file exactly.")
        return Attempt(f"apply failed: {reason}", None, message, details)
    program = to_program(edited, form)
    if program is None:
        return Attempt("syntax", edited, "The edited file's braces do not balance.", details)
    try:
        parser("b").parse(program if program.endswith("\n") else program + "\n")
    except LarkError as error:
        first = str(error).strip().splitlines()[0]
        return Attempt("syntax", edited, f"The edited file does not parse: {first}", details)
    result = isolated(program, "b", "lotml", task.task)
    if result.passed:
        return Attempt("pass", edited, "Every test passes.", details)
    return Attempt(
        "tests fail",
        edited,
        f"The edited file still fails: {first_failure(task.task, result)}",
        details,
    )


INSTRUCTIONS = """Fix the bug with SEARCH/REPLACE blocks and change nothing else. Each block is

<<<<<<< SEARCH
lines copied exactly from the file
=======
the lines that replace them
>>>>>>> REPLACE

A SEARCH section must match the file character for character, whitespace included, and
only once. Answer with the blocks only."""


def system_prompt(form: str) -> str:
    return (
        "You edit lotml code, a programming language described by this reference.\n\n"
        + reference_text(form)
        + "\n\n"
        + INSTRUCTIONS
    )


def first_message(task: EditTask, form: str) -> str:
    return (
        f"In the file `program.lotml` below, {task.failure}\n\n"
        f"```lotml\n{in_form(task.broken, form)}```"
    )


def solve(model: Model, task: EditTask, form: str) -> dict:
    """Up to `TURNS` attempts, each judged strictly, the first also tolerantly."""
    system = system_prompt(form)
    current = in_form(task.broken, form)
    messages = [{"role": "user", "content": first_message(task, form)}]
    record = {"model": model.name, "family": model.family, "form": form, "task": task.task.id}
    turns = []
    for turn in range(1, TURNS + 1):
        try:
            completion = model.chat(system, messages)
        except ModelError as error:
            return record | {"error": str(error), "turns": turns}
        strict = judge(task, form, current, completion.text, tolerant=False)
        entry = {
            "answer": completion.text,
            "outcome": strict.outcome,
            "input_tokens": completion.input_tokens,
            "output_tokens": completion.output_tokens,
            **strict.details,
        }
        if turn == 1:
            entry["tolerant"] = judge(task, form, current, completion.text, tolerant=True).outcome
        turns.append(entry)
        if strict.outcome == "pass":
            break
        if strict.edited is not None:
            current = strict.edited
        messages += [
            {"role": "assistant", "content": completion.text},
            {
                "role": "user",
                "content": f"{strict.feedback}\n\nThe file is now:\n\n```lotml\n{current}```",
            },
        ]
    return record | {
        "error": None,
        "turns": turns,
        "fixed_first": turns[0]["outcome"] == "pass",
        "fixed": turns[-1]["outcome"] == "pass",
        "fixed_first_tolerant": turns[0]["tolerant"] == "pass",
    }


def run(model: Model, tasks: list[EditTask], path: Path, workers: int = 1) -> list[dict]:
    """Every task in both forms, resuming from `path`; failed completions are retried."""
    done: dict[tuple[str, str], dict] = {}
    if path.exists():
        for line in path.read_text(encoding="utf-8").splitlines():
            record = json.loads(line)
            if record.get("error") is None:
                done[(record["task"], record["form"])] = record
    todo = [(t, f) for t in tasks for f in FORMS if (t.task.id, f) not in done]
    path.parent.mkdir(parents=True, exist_ok=True)
    with ThreadPoolExecutor(max_workers=workers) as pool:
        for record in pool.map(lambda job: solve(model, *job), todo):
            with path.open("a", encoding="utf-8") as out:
                out.write(json.dumps(record, ensure_ascii=False) + "\n")
            done[(record["task"], record["form"])] = record
    return [done[(t.task.id, f)] for t in tasks for f in FORMS if (t.task.id, f) in done]


# Report ------------------------------------------------------------------------------


def summarize(rows: list[dict]) -> dict[str, dict]:
    summary: dict[str, dict] = {}
    for model in dict.fromkeys(r["model"] for r in rows):
        mine = [r for r in rows if r["model"] == model and r.get("error") is None]
        entry: dict = {}
        for form in FORMS:
            group = [r for r in mine if r["form"] == form]
            first = [r["turns"][0] for r in group]
            every = [t for r in group for t in r["turns"]]
            entry[form] = {
                "tasks": len(group),
                "fixed_first": sum(r["fixed_first"] for r in group),
                "fixed_first_tolerant": sum(r["fixed_first_tolerant"] for r in group),
                "fixed": sum(r["fixed"] for r in group),
                "outcomes": Counter(t["outcome"] for t in every),
                "rewrites": sum(t["searched"] > 0.5 for t in first),
                "idioms": Counter(name for t in every for name in t["idioms"]),
                "turns": sum(len(r["turns"]) for r in group),
            }
        fixed = {(r["task"], r["form"]): r["fixed"] for r in mine}
        tasks = {t for t, f in fixed if (t, "indented") in fixed and (t, "braces") in fixed}
        entry["pairs"] = len(tasks)
        entry["indented_only"] = sum(
            fixed[(t, "indented")] and not fixed[(t, "braces")] for t in tasks
        )
        entry["braces_only"] = sum(
            fixed[(t, "braces")] and not fixed[(t, "indented")] for t in tasks
        )
        entry["p"] = mcnemar(entry["indented_only"], entry["braces_only"])
        summary[model] = entry
    return summary


def markdown(rows: list[dict], made: int) -> str:
    summary = summarize(rows)
    families = {r["model"]: r["family"] for r in rows}
    lines = [
        "# Editing: the indented form against the braces form",
        "",
        "Generated by `python -m lotml_harness.experiments.editing`. Each task is a file of at",
        f"least {FILE_LINES} non-blank lines of working functions with one silent slip; {made}",
        f"tasks were made. A model has {TURNS} turns of SEARCH/REPLACE edits with feedback.",
        "",
        "## Fixed, paired",
        "",
        "| model | family | pairs | indented, turn 1 | braces, turn 1 | indented, 3 turns "
        "| braces, 3 turns | only indented | only braces | McNemar p |",
        "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    for model, e in summary.items():
        i, b = e["indented"], e["braces"]
        lines.append(
            f"| {model} | {families[model]} | {e['pairs']} "
            f"| {percent(i['fixed_first'], i['tasks'])} | {percent(b['fixed_first'], b['tasks'])} "
            f"| {percent(i['fixed'], i['tasks'])} | {percent(b['fixed'], b['tasks'])} "
            f"| {e['indented_only']} | {e['braces_only']} | {e['p']:.3f} |"
        )
    lines += [
        "",
        "## How edits went",
        "",
        "Every turn's outcome under strict application; the tolerant column counts first",
        "answers that applying with a uniform indentation offset would have fixed.",
        "",
        "| model | form | turns | pass | apply failed | syntax | tests fail | first turn, tolerant "
        "| rewrites | C-family idioms |",
        "| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |",
    ]
    for model, e in summary.items():
        for form in FORMS:
            f = e[form]
            outcomes = f["outcomes"]
            failed = sum(n for k, n in outcomes.items() if k.startswith("apply failed"))
            found = ", ".join(f"{k} {n}" for k, n in f["idioms"].most_common()) or "—"
            lines.append(
                f"| {model} | {form} | {f['turns']} | {outcomes['pass']} | {failed} "
                f"| {outcomes['syntax']} | {outcomes['tests fail']} "
                f"| {f['fixed_first_tolerant']} | {f['rewrites']} | {found} |"
            )
    lines += ["", "## Why edits did not apply", "", "| model | form | reason | turns |"]
    lines += ["| --- | --- | --- | ---: |"]
    for model, e in summary.items():
        for form in FORMS:
            for outcome, n in sorted(e[form]["outcomes"].items()):
                if outcome.startswith("apply failed"):
                    lines.append(
                        f"| {model} | {form} | {outcome.removeprefix('apply failed: ')} | {n} |"
                    )
    return "\n".join(lines) + "\n"


TASKS_FILE = RESULTS / "editing" / "tasks.jsonl"


def save_tasks(made: list[EditTask], path: Path = TASKS_FILE) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    records = [
        {
            "task": t.task.id,
            "original": t.original,
            "broken": t.broken,
            "slip": list(t.slip),
            "failure": t.failure,
        }
        for t in made
    ]
    path.write_text("".join(json.dumps(r, ensure_ascii=False) + "\n" for r in records), "utf-8")


def load_tasks(tasks: dict[str, Task], path: Path = TASKS_FILE) -> list[EditTask]:
    made = []
    for line in path.read_text(encoding="utf-8").splitlines():
        r = json.loads(line)
        made.append(
            EditTask(tasks[r["task"]], r["original"], r["broken"], tuple(r["slip"]), r["failure"])
        )
    return made


def main() -> None:
    options = argparse.ArgumentParser(description=__doc__)
    options.add_argument("--model", action="append", default=[], help="claude:haiku, ...")
    options.add_argument("--workers", type=int, default=1)
    options.add_argument("--make", action="store_true", help="build the tasks from the answers")
    options.add_argument(
        "--more-solutions", type=int, default=0, help="first ask Claude Sonnet for this many more"
    )
    arguments = options.parse_args()
    if arguments.more_solutions:
        more_solutions("claude:sonnet", arguments.more_solutions)
    tasks = {t.id: t for t in build.load()}
    if arguments.make or not TASKS_FILE.exists():
        save_tasks(make_tasks(tasks, solutions(tasks)))
    made = load_tasks(tasks)
    for spec in arguments.model:
        model = from_spec(spec)
        run(model, made, RUNS / f"{model.name}.jsonl", arguments.workers)
    rows = []
    for path in sorted(RUNS.glob("*.jsonl")):
        if path.name != TASKS_FILE.name:
            rows += [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]
    latest = {(r["model"], r["task"], r["form"]): r for r in rows if r.get("error") is None}
    REPORT.write_text(markdown(list(latest.values()), len(made)), encoding="utf-8")
    print(REPORT.read_text(encoding="utf-8"))


if __name__ == "__main__":
    main()
