type Queue[T](var items: [T])

impl Queue:
    fn enqueue(var self, v: T):
        self.items.insert(0, v)

    fn dequeue(var self) -> T?:
        return self.items.pop()

    fn size(self) -> int:
        return len(self.items)

test "queue operations":
    var q = Queue([])
    q.enqueue(1)
    q.enqueue(2)
    q.enqueue(3)
    assert q.size() == 3
    assert q.dequeue()? == 1
    assert q.size() == 2
