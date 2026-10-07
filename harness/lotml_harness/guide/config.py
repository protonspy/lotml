"""The harness guide's configuration, as the harness reads it: the file the compiler's guide tool
reads (specs/guide-tool/), which names the records the guide was trained from."""

import tomllib
from pathlib import Path


def records(config: Path) -> Path | None:
    """The training records `config` names — relative to the file, or absolute — or None when it
    names none or cannot be read."""
    try:
        named = tomllib.loads(config.read_text(encoding="utf-8")).get("records")
    except (OSError, tomllib.TOMLDecodeError):
        return None
    if not isinstance(named, str) or not named:
        return None
    path = Path(named)
    return path if path.is_absolute() else config.parent / path
