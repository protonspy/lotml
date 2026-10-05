from dataclasses import dataclass, field


@dataclass
class Stack[T]:
    items: list[T] = field(default_factory=list)

    def push(self, item: T) -> None:
        self.items.append(item)

    def pop(self) -> T | None:
        if not self.items:
            return None
        return self.items.pop()

    def peek(self) -> T | None:
        return self.items[-1] if self.items else None

    def size(self) -> int:
        return len(self.items)


def test_stack() -> None:
    s: Stack[int] = Stack()
    s.push(1)
    s.push(2)
    assert s.pop() == 2
    assert s.peek() == 1
    assert s.size() == 1
