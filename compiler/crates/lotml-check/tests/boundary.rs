//! The language boundary: a program cannot read an attribute or call a method the checker did
//! not type, and cannot name the compiler's own bookkeeping. These are what keep the Python a
//! backend emits from reaching the interpreter (a security property, not only a type rule).

use lotml_check::check_source;

fn codes(source: &str) -> Vec<&'static str> {
    check_source(source).iter().map(|d| d.code).collect()
}

fn clean(source: &str) {
    let found = check_source(source);
    assert!(
        found.is_empty(),
        "expected no diagnostics in\n{source}\nfound {:#?}",
        found.iter().map(|d| (d.code, &d.message)).collect::<Vec<_>>()
    );
}

#[test]
fn an_attribute_of_an_unknown_type_is_reported() {
    // A lambda whose parameter no context pins has an unknown type; a field of it is unknowable.
    assert_eq!(codes("fn f() -> int:\n    g = lambda x: x.__class__\n    return 0\n"), vec!["E0205"]);
    assert_eq!(codes("fn f() -> int:\n    g = lambda x: x.anything\n    return 0\n"), vec!["E0205"]);
}

#[test]
fn a_method_of_an_unknown_type_is_reported() {
    assert_eq!(codes("fn f() -> int:\n    g = lambda x: x.__class__.mro()\n    return 0\n"), vec!["E0205"]);
    assert_eq!(codes("fn f() -> int:\n    g = lambda h: h.__globals__\n    return 0\n"), vec!["E0205"]);
}

#[test]
fn a_lambda_whose_type_is_known_still_reaches_its_fields() {
    clean("type P(x: int)\n\nfn f(ps: [P]) -> [int]:\n    return sorted([p.x for p in ps])\n");
    clean("fn f(xs: [str]) -> [str]:\n    return sorted(xs, key=lambda s: s.lower())\n");
}

#[test]
fn a_dunder_field_of_a_known_type_is_reported() {
    assert_eq!(codes("type P(x: int)\n\nfn f(p: P) -> int:\n    return p.__class__\n"), vec!["E0205"]);
    assert_eq!(codes("fn f(n: int) -> int:\n    return n.__class__\n"), vec!["E0205"]);
}

#[test]
fn a_name_reserved_for_the_compiler_cannot_be_declared() {
    assert_eq!(codes("fn __rt() -> int:\n    return 1\n"), vec!["E0220"]);
    assert_eq!(codes("fn f() -> int:\n    __x = 1\n    return __x\n"), vec!["E0220"]);
    assert_eq!(codes("type __Secret(x: int)\n\nfn f() -> int:\n    return 1\n"), vec!["E0220"]);
    assert_eq!(codes("fn f(__p: int) -> int:\n    return __p\n"), vec!["E0220"]);
}

#[test]
fn an_ordinary_single_underscore_name_is_fine() {
    clean("fn f(xs: [int]) -> int:\n    var total_ = 0\n    for _ in xs:\n        total_ += 1\n    return total_\n");
}
