type User(name: str, age: int, email: str? = None)

fn find_email(users: [User], name: str) -> str?:
    found = users.find(lambda x: x.name == name)
    if found is not None:
        return found.email
    return None

test "returns the email of the first matching user":
    users = [
        User("ana", 30, "ana@example.com"),
        User("bob", 25),
        User("ana", 41, "other@example.com"),
    ]
    assert (find_email(users, "ana") ?? "") == "ana@example.com"

test "returns nothing when the user has no email":
    users = [User("ana", 30, "ana@example.com"), User(name="bob", age=25)]
    assert find_email(users, "bob") is None

test "returns nothing when there is no such user":
    users = [User("ana", 30, "ana@example.com")]
    assert find_email(users, "zed") is None

test "returns nothing for an empty list":
    users: [User] = []
    assert find_email(users, "ana") is None
