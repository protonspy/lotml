type ParseErr = Empty | NotNumber(text: str) | Negative(value: int)

fn parse_positive(s: str) -> int ! ParseErr:
    if s == "":
        fail Empty
    n = s.to_int() or fail NotNumber(s)
    if n < 0:
        fail Negative(n)
    return n

test "parses a valid number":
    assert parse_positive("42")? == 42
    assert parse_positive("0")? == 0

test "empty string fails":
    assert parse_positive("") == fail Empty

test "non numeric text fails and keeps the text":
    assert parse_positive("abc") == fail NotNumber("abc")
    assert parse_positive("12x") == fail NotNumber("12x")

test "negative number fails and keeps the value":
    assert parse_positive("-5") == fail Negative(-5)
