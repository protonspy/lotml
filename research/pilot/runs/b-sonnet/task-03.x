type ParseErr = Empty | NotNumber(text: str) | Negative(value: int)

fn parse_positive(s: str) -> int ! ParseErr:
    if s == "":
        fail Empty
    n = s.to_int() ?? fail NotNumber(s)
    if n < 0:
        fail Negative(n)
    return n

test "parses a valid number":
    assert parse_positive("42")? == 42
    assert parse_positive("0")? == 0

test "rejects the empty string":
    assert parse_positive("") == Err(Empty)

test "rejects text that is not a number and keeps it":
    assert parse_positive("abc") == Err(NotNumber("abc"))
    assert parse_positive("12x") == Err(NotNumber("12x"))

test "rejects a negative number and keeps the value":
    assert parse_positive("-7") == Err(Negative(-7))
