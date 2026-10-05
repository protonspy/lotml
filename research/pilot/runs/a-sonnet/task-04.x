type User(name: str, age: int, email: str? = none)

fn find_email(users: [User], name: str) -> str?:
    found = users.find(x => x.name == name)
    if found:
        return found.email
    return none

test "returns the email of the first user with the name":
    users = [User("ana", 30, "ana@example.com"), User("bob", 25), User("ana", 41, "other@example.com")]
    assert find_email(users, "ana") == "ana@example.com"

test "returns none when the user has no email":
    users = [User("bob", 25), User("bob", 31, "bob@example.com")]
    assert find_email(users, "bob") == none

test "returns none when there is no such user":
    users = [User("ana", 30, "ana@example.com")]
    assert find_email(users, "zed") == none
    assert find_email([], "ana") == none
