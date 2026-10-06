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
