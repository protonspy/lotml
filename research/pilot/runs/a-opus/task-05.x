# Task 5: generic FIFO queue (two-list implementation)

# `back` receives new items in arrival order; `front` holds items in reverse
# order so the oldest one is popped from its end.
type Queue[T](front: [T], back: [T])

fn new_queue[T]() -> Queue[T]:
    return Queue([], [])

impl Queue[T]:
    fn enqueue(var self, v: T):
        self.back.append(v)

    fn dequeue(var self) -> T?:
        if len(self.front) == 0:
            for x in reversed(self.back):
                self.front.append(x)
            self.back = []
        return self.front.pop()

    fn size(self) -> int:
        return len(self.front) + len(self.back)

test "dequeues in first-in-first-out order":
    var q: Queue[int] = new_queue()
    q.enqueue(1)
    q.enqueue(2)
    q.enqueue(3)
    assert q.size() == 3
    assert (q.dequeue() or -1) == 1
    q.enqueue(4)
    assert (q.dequeue() or -1) == 2
    assert (q.dequeue() or -1) == 3
    assert (q.dequeue() or -1) == 4
    assert q.size() == 0

test "dequeue on an empty queue returns none":
    var q: Queue[str] = new_queue()
    assert q.dequeue() == none
    assert q.size() == 0
