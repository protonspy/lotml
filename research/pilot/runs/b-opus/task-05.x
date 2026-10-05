# Task 5: a generic first-in-first-out queue, built from two stacks.

type Queue[T](inbox: [T], outbox: [T])

impl Queue[T]:
    fn enqueue(var self, v: T):
        self.inbox.append(v)

    fn dequeue(var self) -> T?:
        if len(self.outbox) == 0:
            while len(self.inbox) > 0:
                x = self.inbox.pop()
                if x is not None:
                    self.outbox.append(x)
        return self.outbox.pop()

    fn size(self) -> int:
        return len(self.inbox) + len(self.outbox)

fn new_queue[T]() -> Queue[T]:
    return Queue([], [])

test "dequeues in insertion order":
    var q: Queue[int] = new_queue()
    q.enqueue(1)
    q.enqueue(2)
    q.enqueue(3)
    assert q.size() == 3
    assert (q.dequeue() ?? -1) == 1
    q.enqueue(4)
    assert (q.dequeue() ?? -1) == 2
    assert (q.dequeue() ?? -1) == 3
    assert (q.dequeue() ?? -1) == 4
    assert q.size() == 0

test "dequeue on an empty queue returns nothing":
    var q: Queue[str] = new_queue()
    assert q.dequeue() is None
    assert q.size() == 0
