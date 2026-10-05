type Queue[T](var items: [T])

impl Queue:
    fn enqueue(var self, value: T):
        self.items.append(value)

    fn dequeue(var self) -> T?:
        if len(self.items) > 0:
            result = self.items[0]
            self.items = self.items[1:]
            return result
        return none

    fn size(self) -> int:
        return len(self.items)

test "queue operations":
    var q: Queue[int] = Queue([])
    q.enqueue(1)
    q.enqueue(2)
    q.enqueue(3)
    assert q.size() == 3
    assert q.dequeue() == 1
    assert q.size() == 2
    assert q.dequeue() == 2
    assert q.dequeue() == 3
    assert q.dequeue() == none
