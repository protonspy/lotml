from math import sqrt

type Point(x: f64, y: f64)

fn distance(a: Point, b: Point) -> f64:
    dx = a.x - b.x
    dy = a.y - b.y
    return sqrt(dx * dx + dy * dy)

test "distance of a 3-4-5 triangle":
    assert distance(Point(0.0, 0.0), Point(3.0, 4.0)) == 5.0

test "distance is symmetric":
    a = Point(1.0, 2.0)
    b = Point(4.0, 6.0)
    assert distance(a, b) == distance(b, a)
    assert distance(a, b) == 5.0

test "distance to itself is zero":
    p = Point(-2.5, 7.0)
    assert distance(p, p) == 0.0
