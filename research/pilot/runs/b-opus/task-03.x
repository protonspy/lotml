# Task 3: parse a positive integer, failing on empty, non-numeric and negative input.

type ParseErr = Empty | NotNumber(text: str) | Negative(value: int)

fn parse_positive(s: str) -> int ! ParseErr:
    if s == "":
        fail Empty
    n = s.to_int() ?? fail NotNumber(s)
    if n < 0:
        fail Negative(n)
    return n

test "parses a positive integer":
    assert parse_positive("42")? == 42

test "rejects the empty string":
    assert parse_positive("") == Err(Empty)

test "rejects text that is not a number":
    assert parse_positive("abc") == Err(NotNumber("abc"))

test "rejects a negative number":
    assert parse_positive("-7") == Err(Negative(-7))
