# Task 10: parse "name,age" lines into records

type Person(name: str, age: int)

type ParseErr = BadFieldCount(line: int) | BadAge(line: int)

fn parse_people(text: str) -> [Person] ! ParseErr:
    var people: [Person] = []
    for i, raw in enumerate(text.split("\n")):
        line = raw.strip()
        if line == "":
            continue
        fields = line.split(",")
        if len(fields) != 2:
            fail BadFieldCount(i + 1)
        name = fields[0].strip()
        age = fields[1].strip().to_int() or fail BadAge(i + 1)
        people.append(Person(name, age))
    return people

test "parses records and skips blank lines":
    people = parse_people("ana,30\n\nbob, 25\n")?
    assert len(people) == 2
    assert people[0].name == "ana"
    assert people[0].age == 30
    assert people[1].name == "bob"
    assert people[1].age == 25

test "fails with the line number on a wrong field count":
    assert parse_people("ana,30\nbob\n") == fail BadFieldCount(2)
    assert parse_people("ana,30,x") == fail BadFieldCount(1)

test "fails with the line number on a non-integer age":
    assert parse_people("ana,30\n\nbob,old") == fail BadAge(3)
