type Queue[T](items: [T])

impl Queue[T]:
    fn enqueue(var self, v: T):
        self.items.append(v)

    fn dequeue(var self) -> T?:
        if len(self.items) == 0:
            return none
        head = self.items[0]
        self.items = self.items[1:len(self.items)]
        return head

    fn size(self) -> int:
        return len(self.items)

test "dequeue returns items in insertion order":
    var q: Queue[int] = Queue([])
    q.enqueue(1)
    q.enqueue(2)
    q.enqueue(3)
    assert q.size() == 3
    assert (q.dequeue() or -1) == 1
    assert (q.dequeue() or -1) == 2
    assert q.size() == 1
    assert (q.dequeue() or -1) == 3
    assert q.size() == 0

test "dequeue on an empty queue returns none":
    var q: Queue[str] = Queue([])
    assert q.dequeue() == none
    q.enqueue("a")
    assert (q.dequeue() or "") == "a"
    assert q.dequeue() == none
    assert q.size() == 0
