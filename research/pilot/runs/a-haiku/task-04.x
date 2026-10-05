type User(name: str, age: int, email: str? = none)

fn find_email(users: [User], target_name: str) -> str?:
    for user in users:
        if user.name == target_name:
            return user.email
    return none

test "find email":
    users = [User("alice", 30, "alice@example.com"), User("bob", 25), User("charlie", 35, "charlie@example.com")]
    assert find_email(users, "alice") == "alice@example.com"
    assert find_email(users, "bob") == none
    assert find_email(users, "charlie") == "charlie@example.com"
    assert find_email(users, "unknown") == none
