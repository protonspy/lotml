type Expr = Num(i64) | Add(Expr, Expr) | Mul(Expr, Expr) | Neg(Expr)

fn eval(expr: Expr) -> i64:
    match expr:
        case Num(n):
            return n
        case Add(left, right):
            return eval(left) + eval(right)
        case Mul(left, right):
            return eval(left) * eval(right)
        case Neg(e):
            return 0 - eval(e)

fn render(expr: Expr) -> str:
    match expr:
        case Num(n):
            return str(n)
        case Add(left, right):
            return f"({render(left)} + {render(right)})"
        case Mul(left, right):
            return f"({render(left)} * {render(right)})"
        case Neg(e):
            return f"(-{render(e)})"

test "arithmetic expression":
    expr1 = Num(5)
    expr2 = Add(Num(3), Num(4))
    expr3 = Mul(Num(2), Add(Num(1), Num(2)))
    expr4 = Neg(Num(5))

    assert eval(expr1) == 5
    assert eval(expr2) == 7
    assert eval(expr3) == 6
    assert eval(expr4) == -5

    assert render(expr2) == "(3 + 4)"
    assert render(expr3) == "(2 * (1 + 2))"
