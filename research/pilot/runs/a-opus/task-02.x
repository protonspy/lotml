# Task 2: shapes with area and perimeter

use math.{pi}

type Shape = Circle(radius: f64) | Square(side: f64) | Rectangle(width: f64, height: f64)

fn area(s: Shape) -> f64:
    match s:
        Circle(r):
            return pi * r * r
        Square(a):
            return a * a
        Rectangle(w, h):
            return w * h

fn perimeter(s: Shape) -> f64:
    match s:
        Circle(r):
            return 2.0 * pi * r
        Square(a):
            return 4.0 * a
        Rectangle(w, h):
            return 2.0 * (w + h)

test "area of each shape":
    assert abs(area(Circle(1.0)) - pi) < 0.000001
    assert area(Square(3.0)) == 9.0
    assert area(Rectangle(2.0, 5.0)) == 10.0

test "perimeter of each shape":
    assert abs(perimeter(Circle(1.0)) - 2.0 * pi) < 0.000001
    assert perimeter(Square(3.0)) == 12.0
    assert perimeter(Rectangle(2.0, 5.0)) == 14.0
