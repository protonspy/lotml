type Expr = Number(n: i64) | Sum(left: Expr, right: Expr) | Product(left: Expr, right: Expr) | Negate(Expr)

fn evaluate(e: Expr) -> i64:
    match e:
        Number(n):
            return n
        Sum(left, right):
            return evaluate(left) + evaluate(right)
        Product(left, right):
            return evaluate(left) * evaluate(right)
        Negate(inner):
            return -evaluate(inner)

fn render(e: Expr) -> str:
    match e:
        Number(n):
            return str(n)
        Sum(left, right):
            return f"({render(left)} + {render(right)})"
        Product(left, right):
            return f"({render(left)} * {render(right)})"
        Negate(inner):
            return f"(-{render(inner)})"

test "expression":
    e1 = Number(5)
    assert evaluate(e1) == 5
    assert render(e1) == "5"
    e2 = Sum(Number(2), Number(3))
    assert evaluate(e2) == 5
    assert render(e2) == "(2 + 3)"
    e3 = Product(Number(2), Number(3))
    assert evaluate(e3) == 6
    assert render(e3) == "(2 * 3)"
    e4 = Negate(e2)
    assert evaluate(e4) == -5
    assert render(e4) == "(-(2 + 3))"
