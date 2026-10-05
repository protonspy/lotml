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

test "evaluates expressions":
    assert evaluate(Num(7)) == 7
    assert evaluate(Add(Num(2), Num(3))) == 5
    assert evaluate(Mul(Num(4), Num(5))) == 20
    assert evaluate(Neg(Num(5))) == -5
    assert evaluate(Add(Num(2), Mul(Num(3), Num(4)))) == 14
    assert evaluate(Neg(Add(Num(1), Neg(Num(4))))) == 3

test "renders expressions with parentheses":
    assert render(Num(7)) == "7"
    assert render(Add(Num(2), Num(3))) == "(2 + 3)"
    assert render(Neg(Num(5))) == "(-5)"
    assert render(Add(Num(2), Mul(Num(3), Num(4)))) == "(2 + (3 * 4))"
    assert render(Mul(Add(Num(1), Num(2)), Neg(Num(3)))) == "((1 + 2) * (-3))"
