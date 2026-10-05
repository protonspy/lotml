from dataclasses import dataclass

import pytest


@dataclass
class User:
    name: str
    age: int
    email: str | None = None


class NotFound(Exception):
    pass


class Minor(Exception):
    pass


def find_adult(users: list[User], name: str) -> User:
    for u in users:
        if u.name == name:
            if u.age < 18:
                raise Minor(u.age)
            return u
    raise NotFound(name)


def test_minor_fails() -> None:
    users = [User("ana", 15)]
    with pytest.raises(Minor):
        find_adult(users, "ana")
