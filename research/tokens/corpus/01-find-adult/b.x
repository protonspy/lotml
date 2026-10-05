type User(name: str, age: int, email: str? = None)
type LookupErr = NotFound(name: str) | Minor(age: int)

fn find_adult(users: [User], name: str) -> User ! LookupErr:
    u = users.find(lambda u: u.name == name) ?? fail NotFound(name)
    if u.age < 18:
        fail Minor(u.age)
    return u

test "minor fails":
    users = [User("ana", 15)]
    assert find_adult(users, "ana") == Err(Minor(15))
