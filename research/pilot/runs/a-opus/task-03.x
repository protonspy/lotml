# Task 3: parse a positive integer

type ParseErr = Empty | NotNumber(text: str) | Negative(value: int)

fn parse_positive(s: str) -> int ! ParseErr:
    if s == "":
        fail Empty
    n = s.to_int() or fail NotNumber(s)
    if n < 0:
        fail Negative(n)
    return n

test "parses a positive integer":
    assert parse_positive("42")? == 42

test "rejects an empty string":
    assert parse_positive("") == fail Empty

test "rejects text that is not a number and keeps the text":
    assert parse_positive("abc") == fail NotNumber("abc")

test "rejects a negative number and keeps the value":
    assert parse_positive("-7") == fail Negative(-7)
