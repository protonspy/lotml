# Task 9: arithmetic expressions

type Expr = Num(value: int) | Add(left: Expr, right: Expr) | Mul(left: Expr, right: Expr) | Neg(inner: Expr)

fn evaluate(e: Expr) -> int:
    match e:
        Num(n):
            return n
        Add(a, b):
            return evaluate(a) + evaluate(b)
        Mul(a, b):
            return evaluate(a) * evaluate(b)
        Neg(a):
            return -evaluate(a)

fn render(e: Expr) -> str:
    match e:
        Num(n):
            return str(n)
        Add(a, b):
            return f"({render(a)} + {render(b)})"
        Mul(a, b):
            return f"({render(a)} * {render(b)})"
        Neg(a):
            return f"(-{render(a)})"

test "evaluates a nested expression":
    e = Mul(Add(Num(2), Num(3)), Neg(Num(4)))
    assert evaluate(e) == -20

test "evaluates a single number":
    assert evaluate(Num(7)) == 7

test "renders with parentheses":
    e = Mul(Add(Num(2), Num(3)), Neg(Num(4)))
    assert render(e) == "((2 + 3) * (-4))"
    assert render(Num(5)) == "5"
