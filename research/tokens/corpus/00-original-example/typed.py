from dataclasses import dataclass
from typing import Optional

@dataclass
class User:
    name: str
    age: int
    email: Optional[str] = None

class NotFound(Exception): ...
class Minor(Exception): ...

def find_adult(users: list[User], name: str) -> User:
    for u in users:
        if u.name == name:
            if u.age < 18:
                raise Minor(u.age)
            return u
    raise NotFound(name)
