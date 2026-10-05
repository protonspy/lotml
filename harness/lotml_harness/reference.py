"""The lotml language reference the models read, and the code examples it contains."""

import re
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
