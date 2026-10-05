from dataclasses import dataclass

import pytest


@dataclass
class ParseError(Exception):
    line: int
    reason: str


def parse_line(line: str, number: int) -> tuple[str, str]:
    if "=" not in line:
        raise ParseError(number, "missing '='")
    key, value = line.split("=", 1)
    key = key.strip()
    if not key:
        raise ParseError(number, "empty key")
    return key, value.strip()


def parse_config(text: str) -> dict[str, str]:
    config: dict[str, str] = {}
    for number, line in enumerate(text.splitlines(), start=1):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        key, value = parse_line(line, number)
        config[key] = value
    return config


def test_parse_config() -> None:
    assert parse_config("a = 1\n# note\nb=2") == {"a": "1", "b": "2"}
    with pytest.raises(ParseError) as err:
        parse_config("a = 1\noops")
    assert err.value == ParseError(2, "missing '='")
