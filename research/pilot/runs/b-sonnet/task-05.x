type Queue[T](items: [T])

impl Queue[T]:
    fn enqueue(var self, v: T):
        self.items.append(v)

    fn dequeue(var self) -> T?:
        if len(self.items) == 0:
            return None
        first = self.items[0]
        self.items = self.items[1:len(self.items)]
        return first

    fn size(self) -> int:
        return len(self.items)

test "new queue is empty":
    var q: Queue[int] = Queue([])
    assert q.size() == 0
    assert q.dequeue() is None

test "dequeue returns items in insertion order":
    var q: Queue[int] = Queue([])
    q.enqueue(1)
    q.enqueue(2)
    q.enqueue(3)
    assert q.size() == 3
    assert (q.dequeue() ?? -1) == 1
    assert (q.dequeue() ?? -1) == 2
    assert q.size() == 1
    q.enqueue(4)
    assert (q.dequeue() ?? -1) == 3
    assert (q.dequeue() ?? -1) == 4
    assert q.size() == 0
    assert q.dequeue() is None

test "queue works with strings":
    var q: Queue[str] = Queue([])
    q.enqueue("a")
    q.enqueue("b")
    assert (q.dequeue() ?? "") == "a"
    assert q.size() == 1
