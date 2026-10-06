//! Errors as values (R04, R28; adr:0002): `T ! E`, `?`, `fail`, `??`, and the unit type.

use lotml_check::check_source;
use lotml_diag::Applicability;

fn codes(source: &str) -> Vec<&'static str> {
    check_source(source).iter().map(|d| d.code).collect()
}

fn clean(source: &str) {
    let found = check_source(source);
    assert!(
        found.is_empty(),
        "expected no diagnostics, found {:#?}",
        found.iter().map(|d| (d.code, &d.message)).collect::<Vec<_>>()
    );
}

fn only(source: &str, code: &str) {
    assert_eq!(codes(source), vec![code], "{source}");
}

const PARSE: &str = "type ParseErr = Empty | NotNumber(text: str) | Negative(value: int)\n\nfn parse(s: str) -> int ! ParseErr:\n    if s == \"\":\n        fail Empty\n    n = s.to_int() ?? fail NotNumber(s)\n    if n < 0:\n        fail Negative(n)\n    return n\n\n";

#[test]
fn the_reference_example_checks() {
    clean(&format!(
        "{PARSE}fn double(s: str) -> int ! ParseErr:\n    n = parse(s)?\n    return n * 2\n\nfn check(s: str) -> None ! ParseErr:\n    parse(s)?\n\nfn describe(s: str) -> str:\n    match parse(s):\n        case Ok(n):\n            return f\"number {{n}}\"\n        case Err(NotNumber(text)):\n            return f\"not a number: {{text}}\"\n        case Err(_):\n            return \"invalid\"\n"
    ));
}

#[test]
fn a_question_mark_needs_a_function_that_fails_with_the_same_error() {
    only(&format!("{PARSE}fn f(s: str) -> int:\n    return parse(s)?\n"), "E0214");
    only(&format!("{PARSE}type Other = Bad\n\nfn f(s: str) -> int ! Other:\n    return parse(s)?\n"), "E0214");
}

#[test]
fn fail_needs_the_declared_error_type() {
    only("fn f(x: int) -> int:\n    if x < 0:\n        fail \"negative\"\n    return x\n", "E0214");
    only("type E = Bad\n\nfn f(x: int) -> int ! E:\n    if x < 0:\n        fail \"negative\"\n    return x\n", "E0214");
}

#[test]
fn a_result_used_as_its_value_offers_the_question_mark() {
    let source = format!("{PARSE}fn f(s: str) -> int ! ParseErr:\n    return parse(s) + 1\n");
    let found = check_source(&source);
    assert_eq!(found.iter().map(|d| d.code).collect::<Vec<_>>(), vec!["E0219"]);
    assert_eq!(found[0].fixes[0].applicability, Applicability::MachineApplicable);
    let (text, _) = lotml_diag::apply_fixes(&source, &found);
    assert!(text.contains("return parse(s)? + 1"), "{text}");
}

#[test]
fn a_result_in_a_function_that_cannot_fail_says_to_match() {
    let found = check_source(&format!("{PARSE}fn f(s: str) -> int:\n    n: int = parse(s)\n    return n\n"));
    assert_eq!(found.iter().map(|d| d.code).collect::<Vec<_>>(), vec!["E0219"]);
    assert!(found[0].fixes.is_empty());
    assert!(found[0].notes.iter().any(|n| n.contains("match")));
}

#[test]
fn a_dropped_result_is_reported() {
    only(&format!("{PARSE}fn f(s: str):\n    parse(s)\n"), "E0219");
}

#[test]
fn a_result_compares_with_ok_and_err() {
    clean(&format!(
        "{PARSE}test \"t\":\n    assert parse(\"x\") == Err(NotNumber(\"x\"))\n    assert parse(\"4\") == Ok(4)\n"
    ));
}

#[test]
fn a_question_mark_in_a_test_fails_the_test() {
    clean(&format!("{PARSE}test \"t\":\n    n = parse(\"4\")?\n    assert n == 4\n"));
}

#[test]
fn a_match_on_a_result_covers_ok_and_err() {
    only(
        &format!(
            "{PARSE}fn f(s: str) -> int:\n    match parse(s):\n        case Ok(n):\n            return n\n    return 0\n"
        ),
        "E0206",
    );
}

#[test]
fn coalesce_with_fail_unwraps_an_optional_or_fails() {
    clean(
        "type E = Missing\n\nfn f(d: {str: int}) -> int ! E:\n    v = d.get(\"k\") ?? fail Missing\n    return v + 1\n",
    );
}

#[test]
fn a_function_that_returns_nothing_takes_a_bare_return() {
    clean("fn f(x: int):\n    if x > 0:\n        return\n    print(x)\n");
    only("fn f(x: int) -> int:\n    if x > 0:\n        return\n    return x\n", "E0218");
}
