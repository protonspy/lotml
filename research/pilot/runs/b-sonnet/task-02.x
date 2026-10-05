from math import pi

type Shape = Circle(r: f64) | Square(side: f64) | Rect(w: f64, h: f64)

fn area(s: Shape) -> f64:
    match s:
        case Circle(r):
            return pi * r * r
        case Square(side):
            return side * side
        case Rect(w, h):
            return w * h

fn perimeter(s: Shape) -> f64:
    match s:
        case Circle(r):
            return 2.0 * pi * r
        case Square(side):
            return 4.0 * side
        case Rect(w, h):
            return 2.0 * (w + h)

test "area of each shape":
    assert abs(area(Circle(1.0)) - pi) < 0.000001
    assert abs(area(Circle(2.0)) - 4.0 * pi) < 0.000001
    assert area(Square(3.0)) == 9.0
    assert area(Rect(2.0, 5.0)) == 10.0

test "perimeter of each shape":
    assert abs(perimeter(Circle(1.0)) - 2.0 * pi) < 0.000001
    assert perimeter(Square(3.0)) == 12.0
    assert perimeter(Rect(2.0, 5.0)) == 14.0
