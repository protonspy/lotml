"""The lotml language reference the models read, and the code examples it contains."""

import re
import textwrap
from pathlib import Path

from lotml_harness import ROOT

REFERENCE = ROOT / "reference" / "lotml.md"
TOKEN_BUDGET = 10_000
"""R12: the complete reference fits in this many tokens."""

FENCE = re.compile(r"^```\n(.*?)^```$", re.MULTILINE | re.DOTALL)


def text(path: Path = REFERENCE) -> str:
    return path.read_text(encoding="utf-8")


def examples(source: str) -> list[str]:
    """The code of every unlabelled fenced block, in order."""
    return [match.group(1) for match in FENCE.finditer(source)]


ITEM_KEYWORDS = ("fn ", "type ", "impl ", "trait ", "test ", "from ", "import ", "use ")


def program(example: str) -> str:
    """An example as a whole program: its loose statements go inside a function."""
    items: list[str] = []
    loose: list[str] = []
    current: list[str] | None = None
    for line in example.splitlines():
        if line and not line[0].isspace():
            current = items if line.startswith(ITEM_KEYWORDS) else loose
        (current if current is not None else loose).append(line)
    whole = "\n".join(items)
    if loose:
        whole += "\n\nfn example():\n" + textwrap.indent("\n".join(loose), "    ")
    return whole + "\n"
