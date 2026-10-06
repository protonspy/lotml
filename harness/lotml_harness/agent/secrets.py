"""Secrets kept out of what is committed and what is exported (specs/trace-dataset/ R2.3, R3.7):
the value of every environment variable whose name ends in `_KEY`, `_TOKEN` or `_SECRET`, and the
shapes keys come in.
"""

import os
import re
from typing import Any

SHAPES = re.compile(
    r"sk-[A-Za-z0-9_-]{20,}"
    r"|ghp_[A-Za-z0-9]{30,}"
    r"|hf_[A-Za-z0-9]{30,}"
    r"|AKIA[0-9A-Z]{16}"
    r"|Bearer [A-Za-z0-9._~+/-]{20,}"
    r"|eyJ[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+\.[A-Za-z0-9_-]+"
)
NAMES = ("_KEY", "_TOKEN", "_SECRET")
SHORTEST = 8
"""Values shorter than this are not matched: an empty or short value would match everything."""
REDACTED = "<redacted>"


def values() -> list[str]:
    """The secret values in this environment, longest first, so one inside another goes too."""
    found = {
        value
        for name, value in os.environ.items()
        if name.upper().endswith(NAMES) and len(value) >= SHORTEST
    }
    return sorted(found, key=len, reverse=True)


def scrub(text: str) -> str:
    """`text` with every secret value and key shape replaced by `<redacted>`."""
    for value in values():
        text = text.replace(value, REDACTED)
    return SHAPES.sub(REDACTED, text)


def scrub_value(value: Any) -> Any:
    """A row or report with every string inside it scrubbed."""
    match value:
        case str():
            return scrub(value)
        case list():
            return [scrub_value(v) for v in value]
        case dict():
            return {k: scrub_value(v) for k, v in value.items()}
    return value


def holds_secret(text: str) -> bool:
    return any(value in text for value in values()) or SHAPES.search(text) is not None
