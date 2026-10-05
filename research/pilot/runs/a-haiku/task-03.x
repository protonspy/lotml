type ParseErr = Empty | NotNumber(text: str) | Negative(value: int)

fn parse_positive(s: str) -> int ! ParseErr:
    if s == "":
        fail Empty
    maybe_n = s.to_int()
    if maybe_n:
        n = maybe_n
        if n < 0:
            fail Negative(n)
        return n
    else:
        fail NotNumber(s)

test "parse positive integer":
    assert parse_positive("42")? == 42
    assert parse_positive("0")? == 0
    assert parse_positive("") == fail Empty
    assert parse_positive("abc") == fail NotNumber("abc")
    assert parse_positive("-5") == fail Negative(-5)
