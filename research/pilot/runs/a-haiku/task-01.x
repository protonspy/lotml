use math.sqrt

type Point(x: f64, y: f64)

fn distance(p1: Point, p2: Point) -> f64:
    dx = p2.x - p1.x
    dy = p2.y - p1.y
    return sqrt(dx * dx + dy * dy)

test "distance between points":
    p1 = Point(0.0, 0.0)
    p2 = Point(3.0, 4.0)
    assert distance(p1, p2) == 5.0
    p3 = Point(1.0, 1.0)
    p4 = Point(1.0, 1.0)
    assert distance(p3, p4) == 0.0
