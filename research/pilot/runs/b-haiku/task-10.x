type Person(name: str, age: int)
type ParseErr = MissingField(line: int) | InvalidAge(line: int)

fn parse_people(text: str) -> [Person] ! ParseErr:
    lines = text.split("\n")
    var people = []
    for i, line in enumerate(lines):
        line_num = i + 1
        line = line.strip()

        if line == "":
            continue

        parts = line.split(",")
        if len(parts) != 2:
            fail MissingField(line_num)

        name = parts[0].strip()
        age_str = parts[1].strip()
        age = age_str.to_int() ?? fail InvalidAge(line_num)

        people.append(Person(name, age))

    return people

test "parse people":
    text = "alice,30\nbob,25\n\ncharlie,invalid"
    result = parse_people(text)
    match result:
        case Err(InvalidAge(4)):
            pass
        case _:
            assert false
