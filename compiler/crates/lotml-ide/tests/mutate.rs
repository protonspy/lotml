//! Mutants of a program that checks, made the ways agents break programs
//! (specs/seeded-failures/ R2.1, R2.2): each a single span replaced, named by its operator,
//! its family and the declaration it falls in.

use std::collections::BTreeSet;

use lotml_ide::mutate::{Mutant, mutants};

const PROGRAM: &str = "\
type ParseErr = Empty | Bad(text: str)

type Point(x: f64, y: f64)

fn parse(s: str) -> int ! ParseErr:
    if s == \"\":
        fail Empty
    return s.to_int() ?? fail Bad(s)

fn total(prices: [int]) -> int:
    var sum = 0
    for p in prices:
        sum += p
    return sum

fn double(s: str) -> int ! ParseErr:
    n = parse(s)?
    return n * 2

fn first(xs: [int]) -> int?:
    if len(xs) == 0:
        return None
    return xs[0]

fn norm(p: Point) -> f64:
    return p.x * p.x + p.y * p.y

impl Point:
    fn shifted(self, dx: f64) -> Point:
        return Point(self.x + dx, self.y)

test \"total\":
    assert total([1, 2]) == 3
";

const BREAKING: [&str; 4] = ["names", "types", "calls", "mutability"];

fn codes(text: &str) -> Vec<&'static str> {
    lotml_check::check_source(text).iter().map(|d| d.code).collect()
}

fn breaking() -> Vec<Mutant> {
    mutants(PROGRAM).into_iter().filter(|m| BREAKING.contains(&m.family)).collect()
}

fn one(operator: &str, declaration: &str, original: &str) -> Mutant {
    breaking()
        .into_iter()
        .find(|m| m.operator == operator && m.declaration == declaration && &PROGRAM[m.span.range()] == original)
        .unwrap_or_else(|| panic!("no {operator} mutant of `{original}` in {declaration}"))
}

#[test]
fn the_program_checks() {
    assert!(codes(PROGRAM).is_empty(), "{:?}", codes(PROGRAM));
}

#[test]
fn every_operator_of_the_breaking_families_is_made() {
    let operators: BTreeSet<&str> = breaking().iter().map(|m| m.operator).collect();
    let expected = [
        "misspell-name",
        "misspell-field",
        "int-f64",
        "unwrap-list",
        "wrap-list",
        "add-optional",
        "drop-optional",
        "drop-argument",
        "duplicate-argument",
        "drop-try",
        "drop-var",
    ];
    for operator in expected {
        assert!(operators.contains(operator), "no {operator} mutant among {operators:?}");
    }
}

#[test]
fn every_breaking_mutant_is_one_span_replaced_and_refused_by_check() {
    for mutant in breaking() {
        let mutated = mutant.apply(PROGRAM);
        assert_ne!(mutated, PROGRAM, "{mutant:?} changes nothing");
        assert_eq!(
            mutated,
            format!(
                "{}{}{}",
                &PROGRAM[..mutant.span.start as usize],
                mutant.replacement,
                &PROGRAM[mutant.span.end as usize..]
            )
        );
        assert!(!codes(&mutated).is_empty(), "{mutant:?} still checks:\n{mutated}");
    }
}

#[test]
fn each_family_aims_at_the_diagnostic_agents_meet() {
    let cases = [
        (one("misspell-name", "total", "sum"), "E0201"),
        (one("misspell-field", "norm", "x"), "E0205"),
        (one("drop-var", "total", "var "), "E0301"),
        (one("drop-argument", "Point.shifted", ", self.y"), "E0203"),
    ];
    for (mutant, code) in cases {
        let found = codes(&mutant.apply(PROGRAM));
        assert!(found.contains(&code), "{mutant:?} gives {found:?}, not {code}");
    }
    let unwrapped = codes(&one("drop-try", "double", "?").apply(PROGRAM));
    assert!(unwrapped.iter().any(|c| ["E0219", "E0204"].contains(c)), "{unwrapped:?}");
    let retyped = codes(&one("int-f64", "total", "int").apply(PROGRAM));
    assert!(retyped.contains(&"E0204"), "{retyped:?}");
}

#[test]
fn a_mutant_names_the_innermost_declaration_and_none_falls_in_a_test() {
    let declarations: BTreeSet<String> = mutants(PROGRAM).into_iter().map(|m| m.declaration).collect();
    let expected = ["ParseErr", "Point", "Point.shifted", "double", "first", "norm", "parse", "total"];
    assert_eq!(declarations, expected.iter().map(ToString::to_string).collect());
}

#[test]
fn mutants_are_deterministic_and_in_source_order() {
    let found = mutants(PROGRAM);
    assert_eq!(found, mutants(PROGRAM));
    assert!(found.windows(2).all(|w| w[0].span.start <= w[1].span.start));
    let unique: BTreeSet<(u32, u32, &str)> =
        found.iter().map(|m| (m.span.start, m.span.end, m.replacement.as_str())).collect();
    assert_eq!(unique.len(), found.len(), "a mutant is listed twice");
}

const MEANING: &str = "\
fn clamp(x: int, lo: int, hi: int) -> int:
    if x < lo:
        return lo
    if x > hi:
        return hi
    return x

fn odds(n: int) -> [int]:
    var out: [int] = []
    for i in range(0, n):
        out.append(i * 2 + 1)
    return out

fn both(a: bool, b: bool) -> bool:
    return a and b

fn repeat(s: str, n: int) -> str:
    var out = \"\"
    var k = 0
    while k != n:
        out += s
        k += 1
    return out

fn bounded(x: int) -> int:
    return clamp(x, 0, 10) + 1

fn label(n: int) -> str:
    return repeat(\"ab\", n) + \"!\"

fn ready() -> bool:
    return True
";

fn meaning() -> Vec<Mutant> {
    mutants(MEANING).into_iter().filter(|m| m.family == "meaning").collect()
}

fn changed(declaration: &str, operator: &str) -> Vec<(String, String)> {
    meaning()
        .into_iter()
        .filter(|m| m.declaration == declaration && m.operator == operator)
        .map(|m| (MEANING[m.span.range()].to_string(), m.replacement))
        .collect()
}

#[test]
fn every_operator_of_the_meaning_family_is_made() {
    let operators: BTreeSet<&str> = meaning().iter().map(|m| m.operator).collect();
    let expected = [
        "swap-comparison",
        "swap-arithmetic",
        "swap-logical",
        "move-bound",
        "swap-arguments",
        "negate-condition",
        "change-constant",
        "drop-statement",
    ];
    assert_eq!(operators, expected.into_iter().collect());
}

#[test]
fn every_meaning_mutant_still_checks() {
    assert!(codes(MEANING).is_empty(), "{:?}", codes(MEANING));
    for mutant in meaning() {
        let mutated = mutant.apply(MEANING);
        assert_ne!(mutated, MEANING);
        assert!(codes(&mutated).is_empty(), "{mutant:?} breaks checking: {:?}\n{mutated}", codes(&mutated));
    }
}

#[test]
fn comparisons_arithmetic_and_logic_swap_their_operator_alone() {
    assert_eq!(changed("clamp", "swap-comparison"), [("<".into(), "<=".into()), (">".into(), ">=".into())]);
    assert_eq!(changed("repeat", "swap-comparison"), [("!=".into(), "==".into())]);
    assert_eq!(changed("odds", "swap-arithmetic"), [("+".into(), "-".into())]);
    assert_eq!(changed("both", "swap-logical"), [("and".into(), "or".into())]);
    assert!(changed("label", "swap-arithmetic").is_empty(), "`+` of two strings is not swapped");
}

#[test]
fn a_bound_moves_by_one_and_a_constant_changes() {
    assert_eq!(changed("odds", "move-bound"), [("n".into(), "n + 1".into()), ("n".into(), "n - 1".into())]);
    let constants = changed("odds", "change-constant");
    assert!(constants.contains(&("0".into(), "1".into())), "{constants:?}");
    assert!(constants.contains(&("2".into(), "3".into())), "{constants:?}");
    assert_eq!(changed("ready", "change-constant"), [("True".into(), "False".into())]);
}

#[test]
fn arguments_are_swapped_only_between_two_of_one_type() {
    assert_eq!(
        changed("bounded", "swap-arguments"),
        [("x, 0".into(), "0, x".into()), ("0, 10".into(), "10, 0".into())]
    );
    assert!(changed("label", "swap-arguments").is_empty(), "a str and an int are never swapped");
}

#[test]
fn conditions_are_negated_and_statements_dropped_for_pass() {
    assert_eq!(changed("repeat", "negate-condition"), [("k != n".into(), "not (k != n)".into())]);
    let dropped = changed("repeat", "drop-statement");
    assert_eq!(dropped, [("out += s".into(), "pass".into()), ("k += 1".into(), "pass".into())]);
    assert_eq!(changed("odds", "drop-statement"), [("out.append(i * 2 + 1)".into(), "pass".into())]);
}
