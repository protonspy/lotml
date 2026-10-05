type Shape = Circle(r: f64) | Square(s: f64) | Rect(w: f64, h: f64)

fn area(shape: Shape) -> f64:
    match shape:
        case Circle(r):
            return 3.14159 * r * r
        case Square(s):
            return s * s
        case Rect(w, h):
            return w * h

fn perimeter(shape: Shape) -> f64:
    match shape:
        case Circle(r):
            return 2.0 * 3.14159 * r
        case Square(s):
            return 4.0 * s
        case Rect(w, h):
            return 2.0 * (w + h)

test "area and perimeter":
    circle = Circle(1.0)
    square = Square(2.0)
    rect = Rect(3.0, 4.0)
    assert area(circle) > 3.0 and area(circle) < 3.2
    assert area(square) == 4.0
    assert area(rect) == 12.0
