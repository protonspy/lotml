type ParseErr = Empty | NotNumber(text: str) | Negative(value: int)

fn parse_positive(s: str) -> int ! ParseErr:
    if s == "":
        fail Empty
    n = s.to_int() ?? fail NotNumber(s)
    if n < 0:
        fail Negative(n)
    return n

test "parse positive integer success":
    assert parse_positive("42")? == 42

test "parse positive integer empty":
    assert parse_positive("") == Err(Empty)

test "parse positive integer not number":
    assert parse_positive("abc") == Err(NotNumber("abc"))

test "parse positive integer negative":
    result = parse_positive("-5")
    match result:
        case Err(Negative(-5)):
            pass
        case _:
            assert false
