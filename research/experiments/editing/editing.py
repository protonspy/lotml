"""The editing pilot: models change existing lotml code with search-and-replace edits.

The same twelve programs and tasks are given in two block styles — significant
indentation, and braces — and each model answers with SEARCH/REPLACE blocks, the format
agents' edit tools use. An answer is applied, parsed and run against the program's own
tests plus a hidden one. Two ways to apply:

- strict: the search text must occur exactly once, byte for byte;
- tolerant: a search that matches only up to a uniform indentation offset is applied with
  the replacement shifted by the same offset, as aider's relative-indentation matching does.
"""

import json
import re
import statistics
import sys
from collections import Counter
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent.parent / "indentation"))
from edit_tasks import TASKS, Task
from lark.exceptions import LarkError
from slips import (
    BUDGET,
    from_braces,
    indent,
    missing_return,
    parse,
    to_braces,
)
from transpile import run

HERE = Path(__file__).parent
RESEARCH = HERE.parent.parent
FORMS = ("indented", "braces")
SEARCH, DIVIDER, REPLACE = "<<<<<<< SEARCH", "=======", ">>>>>>> REPLACE"


def program(name: str, form: str) -> str:
    text = (RESEARCH / "tokens" / "corpus" / name / "b.x").read_text(encoding="utf-8")
    return to_braces(text) if form == "braces" else text


# Each edit turns the pilot's variant B spec into the one a form's editor reads.
NAMING = [
    ("# X language reference (variant B)", "# lotml language reference"),
    ("X is a statically", "lotml is a statically"),
    ("Files use the extension `.x`.", "Files use the extension `.lotml`."),
]
BRACES = [
    (
        "Blocks are indented by 4 spaces\nafter a line ending in `:`.",
        (
            "A block is enclosed in `{` and `}`:\nits header line ends with `{`, and a line"
            " holding only `}` closes it. Indentation is not significant."
        ),
    ),
    (
        "Each arm is `case`, a pattern, `:` and a block.",
        "Each arm is `case`, a pattern and a block.",
    ),
    ("`if x is not None:` narrows", "`if x is not None { … }` narrows"),
    (
        "Methods live in `impl Type:` blocks.",
        "Methods live in `impl Type { … }` blocks.",
    ),
    (
        "`impl Trait for Type:` implements them.",
        "`impl Trait for Type { … }` implements them.",
    ),
    ('`test "name":` blocks sit', '`test "name" { … }` blocks sit'),
]
CODE = re.compile(r"```\n(.*?)```", re.DOTALL)


def spec(form: str) -> str:
    text = (RESEARCH / "pilot" / "spec-b.md").read_text(encoding="utf-8")
    for old, new in NAMING + (BRACES if form == "braces" else []):
        if old not in text:
            raise ValueError(f"spec no longer contains {old!r}")
        text = text.replace(old, new)
    if form == "braces":
        text = CODE.sub(lambda m: "```\n" + to_braces(m.group(1)) + "```", text)
    return text


def render_blocks(blocks: list[tuple[str, str]]) -> str:
    return "".join(f"{SEARCH}\n{s}{DIVIDER}\n{r}{REPLACE}\n" for s, r in blocks)


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
            blocks.append(
                ("".join(s + "\n" for s in search), "".join(r + "\n" for r in replace))
            )
            state = None
        elif state == "search":
            search.append(line)
        elif state == "replace":
            replace.append(line)
    return blocks


def line_starts(text: str, search: str) -> list[int]:
    """Where `search` occurs starting at the beginning of a line, as whole lines do."""
    found, at = [], text.find(search)
    while at >= 0:
        if at == 0 or text[at - 1] == "\n":
            found.append(at)
        at = text.find(search, at + 1)
    return found


def stripped(lines: list[str]) -> list[str]:
    return [line.strip() for line in lines]


def occurs(haystack: list[str], needle: list[str]) -> list[int]:
    return [
        i
        for i in range(len(haystack) - len(needle) + 1)
        if haystack[i : i + len(needle)] == needle
    ]


def shifted(line: str, offset: int) -> str:
    if not line.strip():
        return line
    return " " * max(0, indent(line) + offset) + line.lstrip()


def relative_match(lines: list[str], search: list[str]) -> list[tuple[int, int]]:
    """Starts where `search` matches with every line moved by one common offset."""
    anchor = next(j for j, line in enumerate(search) if line.strip())
    found = []
    for start in occurs(stripped(lines), stripped(search)):
        offset = indent(lines[start + anchor]) - indent(search[anchor])
        if all(
            shifted(s, offset) == lines[start + j] or not s.strip()
            for j, s in enumerate(search)
        ):
            found.append((start, offset))
    return found


def apply(text: str, blocks, tolerant: bool = False) -> tuple[str | None, str | None]:
    """Apply `blocks` in order; the result, or None and the reason the edit failed."""
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
            new = (
                [shifted(r, offset) for r in replace.rstrip("\n").split("\n")]
                if replace
                else []
            )
            text = "\n".join(lines[:start] + new + lines[start + len(wanted) :])
            continue
        if len(matches) > 1:
            return None, "ambiguous"
        whitespace = occurs(stripped(lines), stripped(wanted))
        return None, "not found: whitespace" if whitespace else "not found"
    return text, None


def outcome(task: Task, form: str, answer: str, tolerant: bool) -> str:
    """What happens to the program once `answer` is applied: `pass`, or how it failed."""
    edited, reason = apply(program(task.program, form), parse_blocks(answer), tolerant)
    if edited is None:
        return f"apply failed: {reason}"
    hidden = to_braces(task.test) if form == "braces" else task.test
    full = edited.rstrip("\n") + "\n\n" + hidden
    source = from_braces(full) if form == "braces" else full
    if source is None:
        return "syntax"
    try:
        tree = parse(source)
    except LarkError:
        return "syntax"
    if missing_return(tree):
        return "missing return"
    result = run(source, "b", budget=BUDGET)
    if result.error is not None:
        return "syntax" if result.error.startswith("transpile") else "tests fail"
    return "pass" if set(result.tests.values()) == {"pass"} else "tests fail"


def reference_edit(task: Task, form: str) -> str:
    """The reference change as an answer: block by block, or whole-file in braces."""
    if form == "indented":
        return render_blocks(list(task.reference))
    original = program(task.program, "indented")
    edited, _ = apply(original, task.reference)
    return render_blocks([(to_braces(original), to_braces(edited))])


def prepare() -> None:
    """Write what each model reads: the spec, the tasks and the programs, per form."""
    for form in FORMS:
        root = HERE / "prompt" / form
        (root / "programs").mkdir(parents=True, exist_ok=True)
        (root / "spec.md").write_text(spec(form), "utf-8")
        tasks = ["# Editing tasks", ""]
        for task in TASKS:
            (root / "programs" / f"{task.program}.lotml").write_text(
                program(task.program, form), "utf-8"
            )
            tasks.append(f"- `{task.program}.lotml` — {task.instruction}")
        (root / "tasks.md").write_text("\n".join(tasks) + "\n", "utf-8")


def share_searched(task: Task, form: str, answer: str) -> float:
    """How much of the program the answer's SEARCH blocks quote, by non-blank lines."""
    total = sum(1 for line in program(task.program, form).split("\n") if line.strip())
    quoted = sum(
        1
        for search, _ in parse_blocks(answer)
        for line in search.split("\n")
        if line.strip()
    )
    return quoted / total


def main() -> None:
    rows = []
    for run_dir in sorted((HERE / "runs").glob("*-*")):
        form, model = run_dir.name.split("-", 1)
        for task in TASKS:
            path = run_dir / f"{task.program}.edit"
            answer = path.read_text(encoding="utf-8") if path.exists() else None
            rows.append(
                {
                    "form": form,
                    "model": model,
                    "task": task.program,
                    "strict": outcome(task, form, answer, False)
                    if answer is not None
                    else "no answer",
                    "tolerant": outcome(task, form, answer, True)
                    if answer is not None
                    else "no answer",
                    "blocks": len(parse_blocks(answer or "")),
                    "searched": share_searched(task, form, answer or ""),
                }
            )
    (HERE / "results.json").write_text(json.dumps(rows, indent=2) + "\n", "utf-8")
    kinds = sorted({r["strict"] for r in rows} | {r["tolerant"] for r in rows})
    lines = [
        "# Editing pilot",
        "",
        "Generated by `editing.py`. Each cell counts tasks out of twelve; see the module",
        "docstring for strict and tolerant application.",
        "",
        "| form | model | application | " + " | ".join(kinds) + " |",
        "| --- | --- | --- | " + " | ".join("---:" for _ in kinds) + " |",
    ]
    for form, model in sorted({(r["form"], r["model"]) for r in rows}):
        group = [r for r in rows if r["form"] == form and r["model"] == model]
        for mode in ("strict", "tolerant"):
            counts = Counter(r[mode] for r in group)
            lines.append(
                f"| {form} | {model} | {mode} | "
                + " | ".join(str(counts[k]) for k in kinds)
                + " |"
            )
    lines += [
        "",
        "## Edit size",
        "",
        "Share of the program's non-blank lines that an answer's SEARCH blocks quote, and",
        "answers quoting more than half of the program, which amount to a rewrite.",
        "",
        "| form | model | median share | rewrites |",
        "| --- | --- | ---: | ---: |",
    ]
    for form, model in sorted({(r["form"], r["model"]) for r in rows}):
        shares = [
            r["searched"] for r in rows if r["form"] == form and r["model"] == model
        ]
        rewrites = sum(share > 0.5 for share in shares)
        lines.append(
            f"| {form} | {model} | {statistics.median(shares):.0%} | {rewrites} |"
        )
    lines += [
        "",
        "## Every answer that did not pass",
        "",
        "| form | model | task | strict | tolerant |",
    ]
    lines.append("| --- | --- | --- | --- | --- |")
    lines += [
        f"| {r['form']} | {r['model']} | {r['task']} | {r['strict']} | {r['tolerant']} |"
        for r in rows
        if r["strict"] != "pass" or r["tolerant"] != "pass"
    ]
    (HERE / "results.md").write_text("\n".join(lines) + "\n", "utf-8")


if __name__ == "__main__":
    if sys.argv[1:] == ["prepare"]:
        prepare()
    else:
        main()
