type Stack[T](items: [T] = [])

impl Stack[T]:
    fn push(var self, item: T):
        self.items.append(item)

    fn pop(var self) -> T?:
        return self.items.pop()

    fn peek(self) -> T?:
        return self.items.last()

    fn size(self) -> int:
        return len(self.items)

test "stack":
    var s = Stack[int]()
    s.push(1)
    s.push(2)
    assert s.pop() == 2
    assert s.peek() == 1
    assert s.size() == 1
