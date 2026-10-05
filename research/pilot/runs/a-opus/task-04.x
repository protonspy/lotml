# Task 4: email of the first user with a given name

type User(name: str, age: int, email: str? = none)

fn email_of(users: [User], name: str) -> str?:
    user = users.find(u => u.name == name)
    if user:
        return user.email
    return none

test "returns the email of the first matching user":
    users = [User("ana", 30, "ana@example.com"), User("bob", 25), User("ana", 41, "other@example.com")]
    assert (email_of(users, "ana") or "") == "ana@example.com"

test "returns none when no user has the name":
    users = [User("ana", 30, "ana@example.com")]
    assert email_of(users, "zoe") == none

test "returns none when the first matching user has no email":
    users = [User("bob", 25), User("bob", 52, "bob@example.com")]
    assert email_of(users, "bob") == none

test "returns none for an empty list":
    users: [User] = []
    assert email_of(users, "ana") == none
