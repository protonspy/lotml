type User(name: str, age: int, email: str? = none)
type LookupErr = NotFound(name: str) | Minor(age: int)

fn find_adult(users: [User], name: str) -> User ! LookupErr:
    u = users.find(u => u.name == name) or fail NotFound(name)
    if u.age < 18:
        fail Minor(u.age)
    return u

test "minor fails":
    users = [User("ana", 15)]
    assert find_adult(users, "ana") == fail Minor(15)
