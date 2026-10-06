//! Immutability by default and the parameter conventions (R05, R24): what each program must
//! produce, from docs/wiki/pages/memory-model.md and the reference's "Not Python" lines.

use lotml_check::check_source;
use lotml_diag::{Applicability, Severity};

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

/// The text after applying the diagnostic's first fix.
fn fixed(source: &str) -> String {
    let found = check_source(source);
    let (text, applied) = lotml_diag::apply_fixes(source, &found);
    assert!(applied > 0, "no fix applied: {found:#?}");
    text
}

// Locals ---------------------------------------------------------------------------------

#[test]
fn a_local_is_immutable_unless_declared_var() {
    only("fn f() -> int:\n    x = 1\n    x = 2\n    return x\n", "E0301");
    only("fn f() -> int:\n    total = 0\n    total += 1\n    return total\n", "E0301");
    clean("fn f() -> int:\n    var total = 0\n    total += 1\n    return total\n");
}

#[test]
fn reassigning_an_immutable_offers_var_as_a_safe_fix() {
    let found = check_source("fn f() -> int:\n    x = 1\n    x = 2\n    return x\n");
    assert_eq!(found[0].fixes[0].applicability, Applicability::MachineApplicable);
    assert_eq!(
        fixed("fn f() -> int:\n    x = 1\n    x = 2\n    return x\n"),
        "fn f() -> int:\n    var x = 1\n    x = 2\n    return x\n"
    );
}

#[test]
fn mutating_an_immutable_in_place_is_reported() {
    only("fn f() -> [int]:\n    xs = []\n    xs.append(1)\n    return xs\n", "E0302");
    only("fn f() -> {str: int}:\n    d = {}\n    d[\"a\"] = 1\n    return d\n", "E0302");
    only("type P(x: int)\n\nfn f() -> int:\n    p = P(1)\n    p.x = 2\n    return p.x\n", "E0302");
    assert_eq!(
        fixed("fn f() -> [int]:\n    xs = []\n    xs.append(1)\n    return xs\n"),
        "fn f() -> [int]:\n    var xs = []\n    xs.append(1)\n    return xs\n"
    );
}

#[test]
fn a_loop_variable_is_immutable() {
    only("fn f(xs: [int]) -> int:\n    for x in xs:\n        x = 1\n    return 0\n", "E0301");
}

// Parameters -----------------------------------------------------------------------------

#[test]
fn a_parameter_is_read_only() {
    only("fn f(xs: [int]):\n    xs.append(1)\n", "E0302");
    only("fn f(n: int) -> int:\n    n += 1\n    return n\n", "E0301");
}

#[test]
fn a_var_parameter_is_a_local_copy() {
    clean("fn f(var n: int) -> int:\n    n += 1\n    return n\n");
}

const ADD: &str = "fn add(inout xs: [int], v: int):\n    xs.append(v)\n\n";

#[test]
fn an_inout_parameter_is_passed_with_an_ampersand() {
    clean(&format!("{ADD}fn g() -> [int]:\n    var ys = []\n    add(&ys, 1)\n    return ys\n"));
}

#[test]
fn an_inout_argument_without_an_ampersand_gets_the_fix() {
    let source = format!("{ADD}fn g() -> [int]:\n    var ys = []\n    add(ys, 1)\n    return ys\n");
    only(&source, "E0304");
    assert!(fixed(&source).contains("add(&ys, 1)"));
}

#[test]
fn an_ampersand_for_a_parameter_that_is_not_inout_is_reported() {
    let source = "fn g(xs: [int]) -> int:\n    return len(xs)\n\nfn f() -> int:\n    var ys = [1]\n    return g(&ys)\n";
    only(source, "E0305");
    assert!(fixed(source).contains("return g(ys)"));
}

#[test]
fn lending_an_immutable_is_reported() {
    only(&format!("{ADD}fn g() -> [int]:\n    ys = []\n    add(&ys, 1)\n    return ys\n"), "E0303");
}

#[test]
fn an_inout_parameter_may_be_lent_on() {
    clean(&format!("{ADD}fn twice(inout xs: [int]):\n    add(&xs, 1)\n    add(&xs, 2)\n"));
}

#[test]
fn one_place_cannot_be_lent_twice_in_one_call() {
    let swap = "fn swap(inout a: int, inout b: int):\n    t = a\n    a = b\n    b = t\n\n";
    only(&format!("{swap}fn f() -> int:\n    var x = 1\n    swap(&x, &x)\n    return x\n"), "E0307");
    only(&format!("{swap}fn f() -> int:\n    var xs = [1, 2]\n    swap(&xs[0], &xs[0])\n    return xs[0]\n"), "E0307");
    clean(&format!("{swap}fn f() -> int:\n    var x = 1\n    var y = 2\n    swap(&x, &y)\n    return x\n"));
}

#[test]
fn a_value_given_to_a_sink_parameter_cannot_be_used_again() {
    let take = "fn take(sink xs: [int]) -> int:\n    return len(xs)\n\n";
    only(&format!("{take}fn f() -> int:\n    ys = [1]\n    n = take(ys)\n    return n + len(ys)\n"), "E0306");
    clean(&format!("{take}fn f() -> int:\n    ys = [1]\n    return take(ys)\n"));
}

#[test]
fn a_copy_changed_and_dropped_is_a_warning() {
    let found = check_source("fn bump_all(xs: [int]):\n    var mine = xs\n    mine.append(1)\n");
    assert_eq!(found.iter().map(|d| (d.code, d.severity)).collect::<Vec<_>>(), vec![("E0308", Severity::Warning)]);
    clean("fn bumped(xs: [int]) -> [int]:\n    var mine = xs\n    mine.append(1)\n    return mine\n");
    let found = check_source("fn push_one(var xs: [int]):\n    xs.append(1)\n");
    assert_eq!(found.iter().map(|d| (d.code, d.severity)).collect::<Vec<_>>(), vec![("E0308", Severity::Warning)]);
}

// Methods --------------------------------------------------------------------------------

const COUNTER: &str = "type Counter(count: int)\n\nimpl Counter:\n    fn bump(inout self):\n        self.count += 1\n\n    fn get(self) -> int:\n        return self.count\n\n";

#[test]
fn an_inout_self_method_needs_a_var_receiver() {
    only(&format!("{COUNTER}fn f() -> int:\n    c = Counter(0)\n    c.bump()\n    return c.get()\n"), "E0302");
    clean(&format!("{COUNTER}fn f() -> int:\n    var c = Counter(0)\n    c.bump()\n    return c.get()\n"));
}

#[test]
fn a_method_without_inout_self_cannot_change_self() {
    only("type Counter(count: int)\n\nimpl Counter:\n    fn bump(self):\n        self.count += 1\n", "E0302");
}

// Closures -------------------------------------------------------------------------------

#[test]
fn a_lambda_captures_copies() {
    only("fn f(xs: [int]) -> int:\n    var out = []\n    g = lambda x: out.append(x)\n    return len(out)\n", "E0302");
    clean("fn f(xs: [int], k: int) -> [int]:\n    return sorted(xs, key=lambda x: abs(x - k))\n");
}
