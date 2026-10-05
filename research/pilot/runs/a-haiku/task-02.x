use math.pi

type Shape = Circle(r: f64) | Square(side: f64) | Rectangle(w: f64, h: f64)

fn area(s: Shape) -> f64:
    match s:
        Circle(r):
            return pi * r * r
        Square(side):
            return side * side
        Rectangle(w, h):
            return w * h

fn perimeter(s: Shape) -> f64:
    match s:
        Circle(r):
            return 2.0 * pi * r
        Square(side):
            return 4.0 * side
        Rectangle(w, h):
            return 2.0 * (w + h)

test "area and perimeter":
    c = Circle(1.0)
    assert area(c) > 3.14 and area(c) < 3.15
    s = Square(2.0)
    assert area(s) == 4.0
    assert perimeter(s) == 8.0
    r = Rectangle(3.0, 4.0)
    assert area(r) == 12.0
    assert perimeter(r) == 14.0
