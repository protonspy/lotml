type Point(x: f64, y: f64)

fn distance(p1: Point, p2: Point) -> f64:
    dx = p1.x - p2.x
    dy = p1.y - p2.y
    return (dx * dx + dy * dy) ** 0.5

test "distance between points":
    p1 = Point(0.0, 0.0)
    p2 = Point(3.0, 4.0)
    assert distance(p1, p2) == 5.0
