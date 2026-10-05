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

test "evaluate":
    e = Add(Num(1), Mul(Num(2), Num(3)))
    assert evaluate(e) == 7
    assert evaluate(Neg(Add(Num(1), Num(2)))) == -3
    assert evaluate(Num(5)) == 5

test "render":
    e = Add(Num(1), Mul(Num(2), Num(3)))
    assert render(e) == "(1 + (2 * 3))"
    assert render(Neg(Add(Num(1), Num(2)))) == "(-(1 + 2))"
    assert render(Num(5)) == "5"
