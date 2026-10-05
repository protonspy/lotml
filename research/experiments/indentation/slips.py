"""Indentation slips against brace slips: which ones change a program without a word?

An agent editing code can put a line at the wrong depth. With significant indentation the
depth is the whitespace; with braces it is the delimiters, and whitespace is ignored. Each
slip below is applied to a working program and the result is classified:

- `rejected: syntax` — the parser refuses it;
- `rejected: missing return` — it parses, but a function that must return a value can now
  fall off its end, which lotml's type checker rejects;
- `unchanged` — it parses to the same tree;
- `silent: tests catch` — it parses to a different program and its own tests notice;
- `silent: tests pass` — a different program that its tests do not tell apart.
"""

import re
import sys
from collections.abc import Iterator
from dataclasses import dataclass
from pathlib import Path

from lark import Tree
from lark.exceptions import LarkError

sys.path.insert(0, str(Path(__file__).parent.parent / "transpiler"))
from transpile import STRING_RE, bare, parser, run

STEP = 4
BUDGET = 200_000
MASK = re.compile(STRING_RE + r"|#[^\n]*")
HEADER = re.compile(r"^(fn|if|elif|else|for|while|match|case|impl|trait|test)\b")
CONTINUATION = re.compile(r"^(elif|else)\b")


def masked(source: str) -> list[str]:
    """The lines of `source` with strings and comments blanked, positions kept."""
    blank = MASK.sub(lambda m: re.sub(r"[^\n]", " ", m.group()), source)
    return blank.split("\n")


def is_close(line: str) -> bool:
    return line.strip() == "}"


def is_open(line: str) -> bool:
    """A block header ending in `{`, possibly after the `}` it follows: `} else {`."""
    text = line.strip()
    if text.startswith("}"):
        text = text[1:].strip()
    return text.endswith("{") and HEADER.match(text) is not None


def closes(line: str) -> bool:
    """A line that starts by closing a block: `}` alone, or `} else {`."""
    return line.strip().startswith("}") and (is_close(line) or is_open(line))


def without_block_delimiters(line: str) -> str:
    if is_open(line):
        line = line.rstrip()[:-1]
    if line.strip().startswith("}"):
        line = line.replace("}", " ", 1)
    return line


def bracket_rows(source: str, braces: bool = False) -> list[tuple[int, bool]]:
    """Each line's index and whether it starts a statement (no bracket left open).

    With `braces`, block delimiters are not brackets.
    """
    rows, depth = [], 0
    for index, line in enumerate(masked(source)):
        starts = depth == 0 and line.strip() != ""
        if braces and depth == 0 and (closes(line) or is_open(line)):
            line = without_block_delimiters(line)
        depth += sum(line.count(c) for c in "([{") - sum(line.count(c) for c in ")]}")
        rows.append((index, starts))
    return rows


def statement_lines(source: str) -> list[int]:
    return [index for index, starts in bracket_rows(source) if starts]


def indent(line: str) -> int:
    return len(line) - len(line.lstrip())


def shift(source: str, rows, delta: int) -> str | None:
    """Move `rows` by `delta` indentation steps; None if one would pass column zero."""
    lines = source.split("\n")
    for row in rows:
        if lines[row].strip() == "":
            continue
        width = indent(lines[row]) + delta * STEP
        if width < 0:
            return None
        lines[row] = " " * width + lines[row].lstrip()
    return "\n".join(lines)


def line_slips(source: str) -> Iterator[tuple[int, int, str]]:
    for row in statement_lines(source):
        for delta in (-1, 1):
            mutant = shift(source, [row], delta)
            if mutant is not None:
                yield row, delta, mutant


def block_end(lines: list[str], starts: list[int], row: int) -> int:
    """The last line of the statement starting at `row`, nested block included."""
    later = [r for r in starts if r > row and indent(lines[r]) <= indent(lines[row])]
    end = (later[0] if later else len(lines)) - 1
    while end > row and lines[end].strip() == "":
        end -= 1
    return end


def hunk_slips(source: str) -> Iterator[tuple[int, int, str]]:
    """A compound statement moved whole, its relative indentation kept: a misplaced hunk."""
    lines, code = source.split("\n"), masked(source)
    starts = statement_lines(source)
    for row in starts:
        if not code[row].rstrip().endswith(":"):
            continue
        rows = range(row, block_end(lines, starts, row) + 1)
        for delta in (-1, 1):
            mutant = shift(source, rows, delta)
            if mutant is not None:
                yield row, delta, mutant


def to_braces(source: str) -> str:
    """The same program with `{`/`}` blocks, each `}` right after its block's last line."""
    lines, code = source.split("\n"), masked(source)
    starts = set(statement_lines(source))
    out: list[str] = []
    headers: list[int] = []

    def close_down_to(width: int) -> None:
        while headers and headers[-1] >= width:
            last = max(i for i, line in enumerate(out) if line.strip())
            out.insert(last + 1, " " * headers.pop() + "}")

    for index, line in enumerate(lines):
        if index in starts:
            close_down_to(indent(line))
            colon = code[index].rstrip()
            if colon.endswith(":"):
                at = len(colon) - 1
                line = line[:at] + " {" + line[at + 1 :]
                headers.append(indent(lines[index]))
            last = max((i for i, text in enumerate(out) if text.strip()), default=None)
            closing = " " * indent(line) + "}"
            if (
                CONTINUATION.match(line.strip())
                and last is not None
                and out[last] == closing
            ):
                out[last] = closing + " " + line.lstrip()
                continue
        out.append(line)
    close_down_to(0)
    return "\n".join(out)


def from_braces(text: str) -> str | None:
    """Rebuild the indented program from the braces alone; None if they do not balance."""
    lines = text.split("\n")
    depth, out = 0, []
    for (index, starts), code in zip(
        bracket_rows(text, braces=True), masked(text), strict=True
    ):
        line = lines[index]
        if starts and closes(code):
            depth -= 1
            if depth < 0:
                return None
            if is_close(code):
                continue
        body = line.strip()
        if starts and is_open(code):
            start = code.index("}") + 1 if closes(code) else 0
            end = len(code.rstrip()) - 1
            header = line[start:end].strip() + ":" + line[end + 1 :]
            out.append(" " * STEP * depth + header)
            depth += 1
        elif body:
            continuation = not starts and code.strip() != ""
            out.append(" " * STEP * (depth + continuation) + body)
        else:
            out.append("")
    return "\n".join(out) if depth == 0 else None


def consistent(text: str) -> bool:
    """Braces and indentation agree on every statement's depth: the redundant design."""
    depth = 0
    for (index, starts), code in zip(
        bracket_rows(text, braces=True), masked(text), strict=True
    ):
        if not starts:
            continue
        if closes(code):
            depth -= 1
            if depth < 0:
                return False
        if indent(text.split("\n")[index]) != STEP * depth:
            return False
        if is_open(code):
            depth += 1
    return depth == 0


def brace_slips(text: str) -> Iterator[tuple[str, str]]:
    """A `}` moved past its neighbouring line, dropped, or doubled."""
    lines = text.split("\n")
    for row, line in enumerate(lines):
        if not is_close(line):
            continue
        rest = lines[:row] + lines[row + 1 :]
        yield "brace dropped", "\n".join(rest)
        yield "brace doubled", "\n".join(lines[: row + 1] + [line] + lines[row + 1 :])
        above = [r for r in range(row) if lines[r].strip()]
        if above and not is_close(lines[above[-1]]):
            yield "brace up", "\n".join(rest[: above[-1]] + [line] + rest[above[-1] :])
        below = [r for r in range(row + 1, len(lines)) if lines[r].strip()]
        if below and not is_close(lines[below[0]]):
            at = below[0]
            yield "brace down", "\n".join(rest[:at] + [line] + rest[at:])


def always_returns(statements: list) -> bool:
    return any(returns(statement) for statement in statements)


def suite_statements(suite: Tree) -> list:
    return [
        child.children[0] if child.data == "simple_stmt" else child
        for child in suite.children
    ]


def returns(statement) -> bool:
    """Whether control never falls out of `statement`; `fail` and `return` both leave."""
    if not isinstance(statement, Tree):
        return False
    if statement.data == "return_stmt":
        return True
    if statement.data == "expr_stmt":
        value = bare(statement.children[0])
        return (
            statement.children[1] is None
            and getattr(value, "data", None) == "fail_expr"
        )
    if statement.data == "if_stmt":
        _, body, *clauses = statement.children
        branches = [body] + [c.children[-1] for c in clauses if c is not None]
        has_else = any(c is not None and c.data == "else_clause" for c in clauses)
        return has_else and all(always_returns(suite_statements(b)) for b in branches)
    if statement.data == "match_stmt":
        return all(
            always_returns(suite_statements(arm.children[1]))
            for arm in statement.children[1:]
        )
    if statement.data == "while_stmt":
        test = bare(statement.children[0])
        return not isinstance(test, Tree) and test.type == "TRUE"
    return False


def missing_return(tree: Tree) -> bool:
    """Some function declared to return a value can fall off its end."""
    for function in tree.find_data("fn_def"):
        head, suite = function.children
        ret_type = head.children[3]
        if ret_type is None:
            continue
        value = ret_type.children[0].children[0]
        if isinstance(value, Tree) and value.data == "unit_type":
            continue
        if not always_returns(suite_statements(suite)):
            return True
    return False


@dataclass
class Baseline:
    tree: Tree
    tests: dict[str, str]

    @classmethod
    def of(cls, source: str) -> "Baseline":
        return cls(parse(source), run(source, "b", budget=BUDGET).tests)


def parse(source: str) -> Tree:
    return parser("b").parse(source if source.endswith("\n") else source + "\n")


def indented_first_line(source: str) -> bool:
    """Lark's indenter never sees the first code line's indentation; Python rejects it."""
    first = next((line for line in masked(source) if line.strip()), "")
    return indent(first) > 0


def classify(baseline: Baseline, mutant: str | None) -> str:
    if mutant is None or indented_first_line(mutant):
        return "rejected: syntax"
    try:
        tree = parse(mutant)
    except LarkError:
        return "rejected: syntax"
    if tree == baseline.tree:
        return "unchanged"
    if missing_return(tree):
        return "rejected: missing return"
    result = run(mutant, "b", budget=BUDGET)
    if result.error is not None and result.error.startswith("transpile"):
        return "rejected: syntax"
    return (
        "silent: tests catch"
        if result.tests != baseline.tests
        else "silent: tests pass"
    )


KINDS = [
    "rejected: syntax",
    "rejected: missing return",
    "rejected: indentation check",
    "unchanged",
    "silent: tests catch",
    "silent: tests pass",
]


def programs() -> list[tuple[str, str]]:
    research = Path(__file__).parent.parent.parent
    paths = sorted((research / "tokens" / "corpus").glob("*/b.x"))
    paths += sorted((research / "pilot" / "runs").glob("b-*/*.x"))
    named = [
        (f"{p.parent.name}/{p.name}", p.read_text(encoding="utf-8")) for p in paths
    ]
    usable = []
    for name, source in named:
        try:
            parse(source)
        except LarkError:
            continue
        usable.append((name, source))
    return usable


def braces_outcome(baseline: Baseline, mutant: str, checked: bool) -> str:
    if checked and not consistent(mutant):
        return "rejected: indentation check"
    return classify(baseline, from_braces(mutant))


def main() -> None:
    from collections import Counter

    rows: dict[tuple[str, str], Counter] = {}
    examples: list[tuple[str, int, int]] = []

    def count(design: str, slip: str, outcome: str) -> None:
        rows.setdefault((design, slip), Counter())[outcome] += 1

    corpus = programs()
    for name, source in corpus:
        baseline = Baseline.of(source)
        for row, delta, mutant in line_slips(source):
            outcome = classify(baseline, mutant)
            count("indentation", "one line, one level", outcome)
            if outcome == "silent: tests pass":
                examples.append((name, row + 1, delta))
        for _, _, mutant in hunk_slips(source):
            count(
                "indentation", "whole statement, one level", classify(baseline, mutant)
            )
        braces = to_braces(source)
        for _, _, mutant in line_slips_any(braces):
            for checked, design in (
                (False, "braces"),
                (True, "braces + indentation check"),
            ):
                count(
                    design,
                    "one line, one level",
                    braces_outcome(baseline, mutant, checked),
                )
        for kind, mutant in brace_slips(braces):
            slip = (
                "brace moved"
                if kind in ("brace up", "brace down")
                else "brace dropped or doubled"
            )
            for checked, design in (
                (False, "braces"),
                (True, "braces + indentation check"),
            ):
                count(design, slip, braces_outcome(baseline, mutant, checked))

    lines = [
        "# Indentation slips against brace slips",
        "",
        f"Generated by `slips.py` over {len(corpus)} variant B programs that parse: the paired",
        "corpus and the pilot's B runs. Every slip is applied to a working program, one at a",
        "time; `silent` means it parses to a different program. See the module docstring.",
        "",
        "| design | slip | mutants | " + " | ".join(KINDS) + " |",
        "| --- | --- | ---: | " + " | ".join("---:" for _ in KINDS) + " |",
    ]
    for (design, slip), counter in rows.items():
        total = sum(counter.values())
        cells = [f"{counter[k]} ({counter[k] / total:.0%})" for k in KINDS]
        lines.append(f"| {design} | {slip} | {total} | " + " | ".join(cells) + " |")
    lines += [
        "",
        "## Silent slips the tests miss, indentation, one line",
        "",
        "| program | line | direction |",
        "| --- | ---: | --- |",
    ]
    lines += [f"| {n} | {r} | {'in' if d > 0 else 'out'} |" for n, r, d in examples]
    (Path(__file__).parent / "results.md").write_text("\n".join(lines) + "\n", "utf-8")


def line_slips_any(text: str) -> Iterator[tuple[int, int, str]]:
    """Whitespace slips on every non-blank line that is not a lone `}`."""
    for row, line in enumerate(text.split("\n")):
        if not line.strip() or is_close(line):
            continue
        for delta in (-1, 1):
            mutant = shift(text, [row], delta)
            if mutant is not None:
                yield row, delta, mutant


if __name__ == "__main__":
    main()
