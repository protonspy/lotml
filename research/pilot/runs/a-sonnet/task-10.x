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
        age = parts[1].strip().to_int() or fail BadAge(i + 1)
        people.append(Person(parts[0].strip(), age))
    return people

test "parses pairs and skips blank lines":
    people = parse_people("ana,30\n\nbob,25\n")?
    assert len(people) == 2
    assert people[0].name == "ana"
    assert people[0].age == 30
    assert people[1].name == "bob"
    assert people[1].age == 25

test "empty text gives an empty list":
    people = parse_people("")?
    assert len(people) == 0

test "a line with the wrong number of fields fails with its number":
    assert parse_people("ana,30\nbob") == fail BadFields(2)
    assert parse_people("a,1,2") == fail BadFields(1)

test "a non integer age fails with the line number":
    assert parse_people("ana,30\n\nbob,x") == fail BadAge(3)
