//! The canonical formatter (R08): one form, whatever the layout written, with nothing lost.

use lotml_fmt::format;

fn formats(source: &str, expected: &str) {
    let found = format(source).expect("no syntax errors");
    assert_eq!(found, expected);
    assert_eq!(format(&found).expect("still parses"), found, "formatting twice changes nothing");
}

#[test]
fn spacing_indentation_and_blank_lines_are_canonical() {
    formats(
        "fn f(xs:[int],  k:int=2)->int:\n  total=0\n\n\n\n  for i,x in enumerate(xs):\n    total+=x*k\n  return total\ntype P(x:int)\n",
        "fn f(xs: [int], k: int = 2) -> int:\n    total = 0\n\n    for i, x in enumerate(xs):\n        total += x * k\n    return total\n\ntype P(x: int)\n",
    );
}

#[test]
fn parentheses_only_where_precedence_needs_them() {
    formats(
        "fn f(a: int, b: int) -> int:\n    return ((a + b)) * (a - (b * 2)) + (-a) ** 2\n",
        "fn f(a: int, b: int) -> int:\n    return (a + b) * (a - b * 2) + (-a) ** 2\n",
    );
    formats(
        "fn f(x: int?) -> bool:\n    return not (x is None) and (x ?? 0) > 1\n",
        "fn f(x: int?) -> bool:\n    return not (x is None) and (x ?? 0) > 1\n",
    );
}

#[test]
fn tuples_are_parenthesized_except_as_targets() {
    formats(
        "fn f(xs: [(int, str)]) -> (int, str):\n    for n, s in xs:\n        a, b = n, s\n    return 1, \"x\"\n",
        "fn f(xs: [(int, str)]) -> (int, str):\n    for n, s in xs:\n        a, b = (n, s)\n    return (1, \"x\")\n",
    );
}

#[test]
fn comments_stay_where_they_were() {
    formats(
        "# header\nfn f(x: int) -> int:  # why\n    y = x  # same line\n\n    # before the return\n    return y\n# after\n",
        "# header\nfn f(x: int) -> int:  # why\n    y = x  # same line\n\n    # before the return\n    return y\n# after\n",
    );
    formats(
        "fn f(x: int) -> int:\n    if x > 0:\n        return 1\n    else:\n        # negative\n        return 0\n",
        "fn f(x: int) -> int:\n    if x > 0:\n        return 1\n    else:\n        # negative\n        return 0\n",
    );
}

#[test]
fn literals_are_kept_as_written() {
    formats(
        "fn f() -> int:\n    s = 'it''s'\n    t = f\"{s!r:>10}\"\n    return 1_000_000 + 0x_ff\n",
        "fn f() -> int:\n    s = 'it' 's'\n    t = f\"{s!r:>10}\"\n    return 1_000_000 + 0x_ff\n",
    );
}

#[test]
fn items_of_every_kind() {
    formats(
        "from math import sqrt,pi\nimport math\ntype Shape=Circle(r:f64)|Empty\ntrait Area:\n    fn area(self)->f64\n    fn name(self)->str\nimpl Area for Shape:\n    fn area(self)->f64:\n        return 0.0\n    fn name(self)->str:\n        return \"s\"\ntest \"t\":\n    assert 1==1\n",
        "from math import sqrt, pi\nimport math\n\ntype Shape = Circle(r: f64) | Empty\n\ntrait Area:\n    fn area(self) -> f64\n    fn name(self) -> str\n\nimpl Area for Shape:\n    fn area(self) -> f64:\n        return 0.0\n\n    fn name(self) -> str:\n        return \"s\"\n\ntest \"t\":\n    assert 1 == 1\n",
    );
}

#[test]
fn calls_conventions_errors_and_lambdas() {
    formats(
        "fn f(inout xs: [int], sink ys: [int]) -> int ! str:\n    add(&xs, 1)\n    n = parse(\"1\")?\n    m = xs.last() ?? fail \"empty\"\n    zs = sorted(ys, key=lambda y: -y)\n    return sum(x for x in xs) + n + m\n",
        "fn f(inout xs: [int], sink ys: [int]) -> int ! str:\n    add(&xs, 1)\n    n = parse(\"1\")?\n    m = xs.last() ?? fail \"empty\"\n    zs = sorted(ys, key=lambda y: -y)\n    return sum(x for x in xs) + n + m\n",
    );
}

#[test]
fn a_file_with_syntax_errors_is_not_formatted() {
    assert!(format("fn f(:\n").is_err());
}

/// A program in canonical form that reaches every construct: formatting it changes nothing.
const EVERYTHING: &str = r#"from math import sqrt
import math

type Shape = Circle(f64) | Rect(w: f64, h: f64) | Empty

type Box[T](items: [T] = [], label: str? = None)

trait Show:
    fn show(self) -> str

    fn twice(self) -> str:
        return self.show() + self.show()

impl Show for Shape:
    fn show(self) -> str:
        return "shape"

fn first[T: Show](xs: [T], table: {str: (int, f64)}, seen: {int}, d: dyn Show) -> T? ! str:
    var total: int = 0
    count: int = len(xs)
    while total < 10 and not (count == 0):
        total += 1
        if total % 2 == 0:
            continue
        elif total > 8:
            break
        else:
            pass
    for (i, x), y in []:
        print(i, x, y)
    match (total, count):
        case (0, _):
            fail "none"
        case (-1, 2):
            return None
        case _:
            pass
    match Circle(1.0):
        case Circle(r):
            print(r)
        case Rect(w, h):
            print(w * h)
        case Empty:
            print(True, False, None, ())
    squares = {k: v for k, v in [(1, 2)] if k > 0}
    odd = {x for x in [1, 2] if x % 2 == 1}
    total = sum(x * x for x in [1, 2])
    both = list(zip((y for y in [1]), [2]))
    picked = [1, 2, 3][::2] + [1, 2][1:] + [3][:1]
    flipped = -total + ~total + +total
    ok = 1 < 2 <= 3 and 4 not in [5] and total is not None
    either = total ?? 0 ?? 1
    pick = 1 if ok else 2
    f = lambda: 1
    g = lambda a, b: a + b
    move(&total, key=1)
    n = parse("1")?
    s = "a" "b"
    t = (1,)
    assert ok, "message"
    return xs[0] if len(xs) > 0 else None

test "everything":
    assert sqrt(4.0) == 2.0
"#;

#[test]
fn every_construct_is_a_fixed_point() {
    let formatted = format(EVERYTHING).expect("no syntax errors");
    assert_eq!(formatted, EVERYTHING);
}

#[test]
fn a_class_of_an_interface_is_written_as_its_block() {
    let source = "class datetime( date ):\n    year:int\n    fn now()->datetime ! PyError\n    fn isoformat(self)->str ! PyError\n";
    let parsed = lotml_syntax::parse_interface(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    assert_eq!(
        lotml_fmt::item(source, &parsed.module.items[0]),
        "class datetime(date):\n    year: int\n    fn now() -> datetime ! PyError\n    fn isoformat(self) -> str ! PyError\n"
    );
    let empty = "class empty( base )\n";
    let parsed = lotml_syntax::parse_interface(empty);
    assert_eq!(lotml_fmt::item(empty, &parsed.module.items[0]), "class empty(base)\n", "no member, no colon");
}
