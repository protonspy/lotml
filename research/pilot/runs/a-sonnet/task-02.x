use math.{pi}

type Shape = Circle(r: f64) | Square(side: f64) | Rect(w: f64, h: f64)

fn area(s: Shape) -> f64:
    match s:
        Circle(r):
            return pi * r * r
        Square(side):
            return side * side
        Rect(w, h):
            return w * h

fn perimeter(s: Shape) -> f64:
    match s:
        Circle(r):
            return 2.0 * pi * r
        Square(side):
            return 4.0 * side
        Rect(w, h):
            return 2.0 * (w + h)

test "area":
    assert area(Square(3.0)) == 9.0
    assert area(Rect(2.0, 5.0)) == 10.0
    assert area(Circle(2.0)) == pi * 4.0

test "perimeter":
    assert perimeter(Square(3.0)) == 12.0
    assert perimeter(Rect(2.0, 5.0)) == 14.0
    assert perimeter(Circle(1.0)) == 2.0 * pi
