# Task 4: the email of the first user with a given name.

type User(name: str, age: int, email: str? = None)

fn email_of(users: [User], name: str) -> str?:
    for u in users:
        if u.name == name:
            return u.email
    return None

test "returns the email of the first matching user":
    users = [User("ana", 30, "ana@example.com"), User("ana", 41, "other@example.com")]
    assert (email_of(users, "ana") ?? "") == "ana@example.com"

test "returns nothing when the user has no email":
    users = [User("bob", 25), User("bob", 50, "bob@example.com")]
    assert email_of(users, "bob") is None

test "returns nothing when there is no such user":
    users = [User("ana", 30, "ana@example.com")]
    assert email_of(users, "carl") is None
