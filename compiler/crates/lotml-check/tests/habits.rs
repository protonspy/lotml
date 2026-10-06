//! The neighbours' habits (R25): what a model trained on Python, Rust or TypeScript writes by
//! reflex. Each is reported with its code, and applying the machine fixes leaves a program that
//! checks clean.

use lotml_check::check_source;
use lotml_diag::apply_fixes;

/// The codes reported, and the program after every machine-applicable fix.
fn repaired(source: &str) -> (Vec<&'static str>, String) {
    let found = check_source(source);
    let codes = found.iter().map(|d| d.code).collect();
    let (text, _) = apply_fixes(source, &found);
    (codes, text)
}

/// `source` reports exactly `codes`, and its fixes give `expected`, which checks clean.
fn habit(source: &str, codes: &[&str], expected: &str) {
    let (found, text) = repaired(source);
    assert_eq!(found, codes, "{source}");
    assert_eq!(text, expected);
    let left = check_source(&text);
    assert!(left.is_empty(), "after the fix: {:#?}", left.iter().map(|d| (d.code, &d.message)).collect::<Vec<_>>());
}

#[test]
fn mutating_an_immutable() {
    habit(
        "fn f(xs: [int]) -> [int]:\n    out = []\n    for x in xs:\n        out.append(x)\n    return out\n",
        &["E0302"],
        "fn f(xs: [int]) -> [int]:\n    var out = []\n    for x in xs:\n        out.append(x)\n    return out\n",
    );
    habit(
        "fn f() -> int:\n    n = 0\n    n += 1\n    return n\n",
        &["E0301"],
        "fn f() -> int:\n    var n = 0\n    n += 1\n    return n\n",
    );
}

#[test]
fn truthiness() {
    habit(
        "fn f(xs: [int], s: str, n: int) -> int:\n    if not xs:\n        return 0\n    if s:\n        return 1\n    if n:\n        return 2\n    return 3\n",
        &["E0208", "E0208", "E0208"],
        "fn f(xs: [int], s: str, n: int) -> int:\n    if len(xs) == 0:\n        return 0\n    if s != \"\":\n        return 1\n    if n != 0:\n        return 2\n    return 3\n",
    );
}

#[test]
fn raise() {
    // `raise` becomes `fail`; the error type still has to be declared, which no fix can guess.
    let (codes, text) = repaired(
        "type E = Negative\n\nfn f(x: int) -> int ! E:\n    if x < 0:\n        raise Negative\n    return x\n",
    );
    assert_eq!(codes, vec!["E0107"]);
    assert!(text.contains("        fail Negative\n"), "{text}");
    assert!(check_source(&text).is_empty());
}

#[test]
fn an_argument_used_as_a_reference() {
    let add = "fn add(inout xs: [int], v: int):\n    xs.append(v)\n\n";
    habit(
        &format!("{add}fn f() -> [int]:\n    var ys = []\n    add(ys, 1)\n    return ys\n"),
        &["E0304"],
        &format!("{add}fn f() -> [int]:\n    var ys = []\n    add(&ys, 1)\n    return ys\n"),
    );
}

#[test]
fn else_if() {
    habit(
        "fn f(x: int) -> int:\n    if x < 0:\n        return 0\n    else if x > 5:\n        return 5\n    return x\n",
        &["E0105"],
        "fn f(x: int) -> int:\n    if x < 0:\n        return 0\n    elif x > 5:\n        return 5\n    return x\n",
    );
}

#[test]
fn def() {
    habit("def f(x: int) -> int:\n    return x\n", &["E0101"], "fn f(x: int) -> int:\n    return x\n");
}

#[test]
fn constants_from_other_languages() {
    habit(
        "fn f(x: int) -> bool:\n    if x > 0:\n        return true\n    return false\n",
        &["E0201", "E0201"],
        "fn f(x: int) -> bool:\n    if x > 0:\n        return True\n    return False\n",
    );
    habit("fn f() -> int?:\n    return null\n", &["E0201"], "fn f() -> int?:\n    return None\n");
}

#[test]
fn let_and_const() {
    habit(
        "fn f(xs: [int]) -> int:\n    let n = len(xs)\n    return n\n",
        &["E0110"],
        "fn f(xs: [int]) -> int:\n    n = len(xs)\n    return n\n",
    );
    habit(
        "fn f() -> int:\n    let mut n = 0\n    n += 1\n    return n\n",
        &["E0110"],
        "fn f() -> int:\n    var n = 0\n    n += 1\n    return n\n",
    );
}

#[test]
fn operators_from_other_languages() {
    habit(
        "fn f(x: int) -> bool:\n    return x > 0 && x < 5 || !(x == 3)\n",
        &["E0111", "E0111", "E0111"],
        "fn f(x: int) -> bool:\n    return x > 0 and x < 5 or not (x == 3)\n",
    );
    habit(
        "fn f() -> int:\n    var x = 0\n    x++\n    return x\n",
        &["E0111"],
        "fn f() -> int:\n    var x = 0\n    x += 1\n    return x\n",
    );
}

#[test]
fn typing_spellings() {
    habit(
        "from typing import List, Optional\n\nfn f(xs: List[int], d: Dict[str, int]) -> Optional[int]:\n    return None\n",
        &["E0216", "E0202", "E0202", "E0202"],
        "\nfn f(xs: [int], d: {str: int}) -> int?:\n    return None\n",
    );
}

#[test]
fn a_statement_from_python_does_not_cascade() {
    let found =
        check_source("fn f(x: int) -> int:\n    try:\n        return x\n    except ValueError:\n        return 0\n");
    assert_eq!(found.iter().map(|d| d.code).collect::<Vec<_>>(), vec!["E0108", "E0108"]);
}
