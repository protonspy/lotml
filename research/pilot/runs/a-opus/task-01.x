# Task 1: 2D point and Euclidean distance

use math.{sqrt}

type Point(x: f64, y: f64)

fn distance(a: Point, b: Point) -> f64:
    dx = a.x - b.x
    dy = a.y - b.y
    return sqrt(dx * dx + dy * dy)

test "distance of a 3-4-5 triangle":
    assert distance(Point(0.0, 0.0), Point(3.0, 4.0)) == 5.0

test "distance is symmetric":
    p = Point(1.0, 2.0)
    q = Point(-2.0, 6.0)
    assert distance(p, q) == distance(q, p)

test "distance to self is zero":
    p = Point(x=1.5, y=-2.0)
    assert distance(p, p) == 0.0
