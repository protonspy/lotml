type ParseError(line: int, reason: str)

fn parse_line(line: str, number: int) -> (str, str) ! ParseError:
    pair = line.split_once("=") or fail ParseError(number, "missing '='")
    key = pair[0].strip()
    if key == "":
        fail ParseError(number, "empty key")
    return (key, pair[1].strip())

fn parse_config(text: str) -> {str: str} ! ParseError:
    var config: {str: str} = {}
    for number, line in enumerate(text.splitlines(), start=1):
        trimmed = line.strip()
        if trimmed == "" or trimmed.startswith("#"):
            continue
        key, value = parse_line(trimmed, number)?
        config[key] = value
    return config

test "parse config":
    assert parse_config("a = 1\n# note\nb=2")? == {"a": "1", "b": "2"}
    assert parse_config("a = 1\noops") == fail ParseError(2, "missing '='")
