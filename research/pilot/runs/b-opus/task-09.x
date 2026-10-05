# Task 9: arithmetic expressions, evaluated and rendered.

type Expr = Num(value: int) | Add(left: Expr, right: Expr) | Mul(left: Expr, right: Expr) | Neg(inner: Expr)

fn evaluate(e: Expr) -> int:
    match e:
        case Num(v):
            return v
        case Add(a, b):
            return evaluate(a) + evaluate(b)
        case Mul(a, b):
            return evaluate(a) * evaluate(b)
        case Neg(x):
            return -evaluate(x)

fn render(e: Expr) -> str:
    match e:
        case Num(v):
            return str(v)
        case Add(a, b):
            return f"({render(a)} + {render(b)})"
        case Mul(a, b):
            return f"({render(a)} * {render(b)})"
        case Neg(x):
            return f"(-{render(x)})"

test "evaluates an expression":
    e = Mul(Add(Num(1), Num(2)), Neg(Num(3)))
    assert evaluate(e) == -9

test "renders an expression with parentheses":
    e = Mul(Add(Num(1), Num(2)), Neg(Num(3)))
    assert render(e) == "((1 + 2) * (-3))"

test "a bare number":
    assert evaluate(Num(7)) == 7
    assert render(Num(7)) == "7"
