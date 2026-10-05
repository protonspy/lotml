type User(name: str, age: int, email: str? = None)

fn find_email(users: [User], name: str) -> str?:
    for user in users:
        if user.name == name:
            return user.email
    return None

test "find email":
    users = [User("Alice", 30, "alice@example.com"), User("Bob", 25, None), User("Charlie", 35, "charlie@example.com")]
    assert find_email(users, "Alice") == "alice@example.com"
    assert find_email(users, "Bob") is None
    assert find_email(users, "Unknown") is None
