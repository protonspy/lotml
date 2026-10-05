"""The braces form of lotml, for the editing comparison, and the slips an edit can make.

The braces form is the same program with each block's `:` replaced by `{` and a line
holding `}` after the block's last line; `elif` and `else` follow their `}`. Indentation in
it means nothing: `from_braces` rebuilds the indented program from the delimiters alone,
which is how a braces program is parsed and run. Grown from research/experiments/indentation.
"""

import re
from collections.abc import Iterator

from lotml_harness.lang.grammar import STRING_RE

STEP = 4
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
    """Each statement line moved one level out (-1) or in (+1): an edit's slip."""
    for row in statement_lines(source):
        for delta in (-1, 1):
            mutant = shift(source, [row], delta)
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
            if CONTINUATION.match(line.strip()) and last is not None and out[last] == closing:
                out[last] = closing + " " + line.lstrip()
                continue
        out.append(line)
    close_down_to(0)
    return "\n".join(out)


def from_braces(text: str) -> str | None:
    """Rebuild the indented program from the braces alone; None if they do not balance."""
    lines = text.split("\n")
    depth, out = 0, []
    for (index, starts), code in zip(bracket_rows(text, braces=True), masked(text), strict=True):
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
