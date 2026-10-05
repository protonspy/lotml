type User(name: str, age: int)

type ParseErr = InvalidFields(line: int) | InvalidAge(line: int)

fn parse_users(text: str) -> [User] ! ParseErr:
    lines = text.split("\n")
    var users = []
    for line_num, line in enumerate(lines):
        line = line.strip()
        if line == "":
            continue
        parts = line.split(",")
        if len(parts) != 2:
            fail InvalidFields(line_num + 1)
        name = parts[0].strip()
        age_str = parts[1].strip()
        maybe_age = age_str.to_int()
        if maybe_age:
            age = maybe_age
            users.append(User(name, age))
        else:
            fail InvalidAge(line_num + 1)
    return users

test "parse users":
    text = "alice,30\nbob,25\n\ncharlie,35"
    result = parse_users(text)?
    assert len(result) == 3
    assert result[0].name == "alice"
    assert result[0].age == 30
    assert result[2].name == "charlie"
    assert result[2].age == 35
    text_invalid = "alice,30\ninvalid"
    assert parse_users(text_invalid) == fail InvalidFields(2)
    text_bad_age = "alice,notanumber"
    assert parse_users(text_bad_age) == fail InvalidAge(1)
