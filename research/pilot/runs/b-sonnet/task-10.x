type Person(name: str, age: int)

type LineErr = BadFields(line: int) | BadAge(line: int)

fn parse_people(text: str) -> [Person] ! LineErr:
    var people: [Person] = []
    for i, line in enumerate(text.split("\n")):
        if line.strip() == "":
            continue
        parts = line.split(",")
        if len(parts) != 2:
            fail BadFields(i + 1)
        age = parts[1].strip().to_int() ?? fail BadAge(i + 1)
        people.append(Person(parts[0].strip(), age))
    return people

test "parses pairs and skips blank lines":
    people = parse_people("ana,30\n\nbob,25\n")?
    assert people == [Person("ana", 30), Person("bob", 25)]

test "empty text gives an empty list":
    people = parse_people("")?
    assert len(people) == 0

test "fails with the line number when the field count is wrong":
    assert parse_people("ana,30\nbob") == Err(BadFields(2))
    assert parse_people("ana,30,1") == Err(BadFields(1))

test "fails with the line number when the age is not an integer":
    assert parse_people("ana,x") == Err(BadAge(1))
    assert parse_people("ana,30\n\nbob,old") == Err(BadAge(3))
