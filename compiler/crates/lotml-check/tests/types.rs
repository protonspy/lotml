//! The type checker, from the requirements: each case is a program and the codes it must
//! produce, in order. A clean program produces none.

use lotml_check::check_source;

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

// R02: signatures are annotated; locals are inferred ------------------------------------

#[test]
fn annotated_signatures_and_inferred_locals_check() {
    clean("fn add(a: int, b: int) -> int:\n    total = a + b\n    return total\n");
    clean("fn f(xs: [int]) -> [int]:\n    var out = []\n    for x in xs:\n        out.append(x * 2)\n    return out\n");
    clean("fn f() -> {str: int}:\n    var d = {}\n    d[\"a\"] = 1\n    return d\n");
}

#[test]
fn an_annotated_local_holds_its_type() {
    only("fn f() -> int:\n    x: int = \"a\"\n    return 1\n", "E0204");
    only("fn f() -> int:\n    var x: int = 1.5\n    return x\n", "E0204");
    clean("fn f() -> [int]:\n    var xs: [int] = []\n    ys: {str: int} = {}\n    return xs\n");
}

#[test]
fn a_parameter_without_a_type_is_reported() {
    only("fn f(x) -> int:\n    return 1\n", "E0217");
}

#[test]
fn mismatched_types_are_reported_with_expected_and_found() {
    let found = check_source("fn f() -> int:\n    return \"a\"\n");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code, "E0204");
    assert!(found[0].message.contains("int") && found[0].message.contains("str"));
}

#[test]
fn integers_and_floats_do_not_mix_except_through_division() {
    only("fn f() -> f64:\n    return 1 + 2.0\n", "E0204");
    clean("fn f(a: int, b: int) -> f64:\n    return a / b\n");
    clean("fn f(a: int) -> f64:\n    return float(a) + 2.0\n");
    clean("fn f(a: int, b: int) -> int:\n    return a // b + a % b\n");
}

#[test]
fn unresolved_names_list_the_closest_names_in_scope() {
    let found = check_source("fn f(count: int) -> int:\n    return cuont + 1\n");
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].code, "E0201");
    assert!(found[0].alternatives.contains(&"count".to_string()));
}

#[test]
fn unknown_types_are_reported() {
    only("fn f(x: Strng) -> int:\n    return 1\n", "E0202");
}

#[test]
fn calls_check_arity_and_argument_types() {
    only("fn g(a: int) -> int:\n    return a\n\nfn f() -> int:\n    return g(1, 2)\n", "E0203");
    only("fn g(a: int) -> int:\n    return a\n\nfn f() -> int:\n    return g(\"x\")\n", "E0204");
    clean("fn g(a: int, b: int = 2) -> int:\n    return a + b\n\nfn f() -> int:\n    return g(1) + g(1, b=3)\n");
}

#[test]
fn functions_may_be_used_before_their_declaration() {
    clean("fn f() -> int:\n    return g()\n\nfn g() -> int:\n    return 1\n");
}

// R03: records, sum types and exhaustive match -------------------------------------------

const SHAPES: &str = "type Shape = Circle(r: f64) | Rect(w: f64, h: f64) | Empty\n\n";

#[test]
fn records_build_by_position_or_name_and_read_their_fields() {
    clean(
        "type User(name: str, age: int, email: str? = None)\n\nfn f() -> int:\n    u = User(\"ana\", 30)\n    v = User(name=\"bo\", age=2)\n    return u.age + v.age\n",
    );
}

#[test]
fn an_unknown_field_lists_the_fields() {
    let found = check_source("type P(x: f64, y: f64)\n\nfn f(p: P) -> f64:\n    return p.z\n");
    assert_eq!(found.iter().map(|d| d.code).collect::<Vec<_>>(), vec!["E0205"]);
    assert_eq!(found[0].alternatives, vec!["x".to_string(), "y".to_string()]);
}

#[test]
fn a_complete_match_checks() {
    clean(&format!(
        "{SHAPES}fn area(s: Shape) -> f64:\n    match s:\n        case Circle(r):\n            return 3.14 * r * r\n        case Rect(w, h):\n            return w * h\n        case Empty:\n            return 0.0\n"
    ));
}

#[test]
fn a_match_missing_variants_lists_them() {
    let found = check_source(&format!(
        "{SHAPES}fn area(s: Shape) -> f64:\n    match s:\n        case Circle(r):\n            return r\n    return 0.0\n"
    ));
    assert_eq!(found.iter().map(|d| d.code).collect::<Vec<_>>(), vec!["E0206"]);
    assert_eq!(found[0].alternatives, vec!["Rect".to_string(), "Empty".to_string()]);
}

#[test]
fn a_wildcard_completes_a_match() {
    clean(&format!(
        "{SHAPES}fn round(s: Shape) -> bool:\n    match s:\n        case Circle(_):\n            return True\n        case _:\n            return False\n"
    ));
}

#[test]
fn positional_variant_fields_and_recursion() {
    clean(
        "type Expr = Num(int) | Add(Expr, Expr) | Neg(Expr)\n\nfn eval(e: Expr) -> int:\n    match e:\n        case Num(n):\n            return n\n        case Add(a, b):\n            return eval(a) + eval(b)\n        case Neg(inner):\n            return -eval(inner)\n",
    );
}

#[test]
fn a_pattern_of_the_wrong_shape_is_reported() {
    only(
        &format!(
            "{SHAPES}fn f(s: Shape) -> f64:\n    match s:\n        case Circle(a, b):\n            return a\n        case _:\n            return 0.0\n"
        ),
        "E0211",
    );
}

// R04: optionals ------------------------------------------------------------------------

#[test]
fn an_optional_must_be_checked_before_use() {
    only("fn f(x: int?) -> int:\n    return x + 1\n", "E0207");
    clean("fn f(x: int?) -> int:\n    if x is not None:\n        return x + 1\n    return 0\n");
    clean("fn f(x: int?) -> int:\n    if x is None:\n        return 0\n    return x + 1\n");
    clean("fn f(x: int?) -> int:\n    return x ?? 0\n");
    clean("fn f(x: int?) -> bool:\n    return x is not None and x > 3\n");
}

#[test]
fn coalescing_a_value_that_is_not_optional_is_reported() {
    only("fn f(x: int) -> int:\n    return x ?? 0\n", "E0215");
}

#[test]
fn coalescing_an_optional_already_tested_is_only_a_warning() {
    let found = check_source("fn f(x: int?) -> int:\n    if x is None:\n        return 0\n    return x ?? 0\n");
    assert_eq!(
        found.iter().map(|d| (d.code, d.severity)).collect::<Vec<_>>(),
        vec![("E0215", lotml_diag::Severity::Warning)]
    );
}

#[test]
fn an_optional_given_a_value_on_every_path_is_known_to_hold_one() {
    clean(
        "fn rolling_max(xs: [int]) -> [int]:\n    var out = []\n    var best = None\n    for x in xs:\n        if best is None or x > best:\n            best = x\n        out.append(best)\n    return out\n",
    );
    clean(
        "fn f(xs: [int]) -> int:\n    var x: int? = None\n    var i = 0\n    while x is None:\n        x = xs[i]\n        i += 1\n    return x\n",
    );
    only("fn f(flag: bool) -> int:\n    var x: int? = None\n    if flag:\n        x = 1\n    return x\n", "E0207");
}

#[test]
fn a_test_does_not_outlive_an_assignment_in_a_loop() {
    only(
        "fn f(x: int?) -> int:\n    var y = x\n    if y is not None:\n        for i in range(3):\n            z = y + i\n            y = None\n    return 0\n",
        "E0207",
    );
}

// R25: conditions are bool ------------------------------------------------------------

#[test]
fn truthiness_is_reported_with_the_fix_for_the_type() {
    let found = check_source("fn f(xs: [int]) -> int:\n    if xs:\n        return 1\n    return 0\n");
    assert_eq!(found.iter().map(|d| d.code).collect::<Vec<_>>(), vec!["E0208"]);
    assert_eq!(found[0].fixes[0].edits[0].replacement, "len(xs) > 0");
}

// R06: generics and traits ------------------------------------------------------------

#[test]
fn generic_functions_instantiate_per_call() {
    clean(
        "fn first[T](xs: [T]) -> T?:\n    if len(xs) == 0:\n        return None\n    return xs[0]\n\nfn f() -> int:\n    return (first([1, 2]) ?? 0) + len(first([\"a\"]) ?? \"\")\n",
    );
}

#[test]
fn traits_bound_generics_and_dyn_values() {
    let program = "trait Show:\n    fn show(self) -> str\n\ntype Dog(name: str)\ntype Car(model: str)\n\nimpl Show for Dog:\n    fn show(self) -> str:\n        return self.name\n\nimpl Show for Car:\n    fn show(self) -> str:\n        return self.model\n\nfn all[T: Show](items: [T]) -> str:\n    return \", \".join([i.show() for i in items])\n\nfn mixed(items: [dyn Show]) -> str:\n    return \", \".join([i.show() for i in items])\n\nfn f() -> str:\n    return all([Dog(\"a\")]) + mixed([Dog(\"a\"), Car(\"b\")])\n";
    clean(program);
}

#[test]
fn a_type_without_the_bound_trait_is_reported() {
    only(
        "trait Show:\n    fn show(self) -> str\n\ntype P(x: int)\n\nfn all[T: Show](items: [T]) -> int:\n    return 0\n\nfn f() -> int:\n    return all([P(1)])\n",
        "E0213",
    );
}

#[test]
fn methods_and_static_functions_in_impl() {
    clean(
        "type Counter(count: int)\n\nimpl Counter:\n    fn new() -> Counter:\n        return Counter(0)\n\n    fn get(self) -> int:\n        return self.count\n\n    fn bump(inout self):\n        self.count += 1\n\nfn f() -> int:\n    var c = Counter.new()\n    c.bump()\n    return c.get()\n",
    );
}

// R33: todo() is a value of every type ------------------------------------------------

#[test]
fn todo_fills_any_hole() {
    clean(
        "type Plan(steps: [str])\n\nfn schedule(n: int) -> Plan:\n    return todo()\n\nfn g() -> int:\n    x: str = todo()\n    return len(x)\n",
    );
}

// Control flow --------------------------------------------------------------------------

#[test]
fn a_function_that_may_not_return_is_reported() {
    only("fn f(x: int) -> int:\n    if x > 0:\n        return 1\n", "E0209");
    clean("fn f(x: int) -> int:\n    if x > 0:\n        return 1\n    else:\n        return 2\n");
    clean("fn f(x: int) -> int:\n    while True:\n        return x\n");
}

#[test]
fn returning_a_value_from_a_unit_function_is_reported() {
    only("fn f(x: int):\n    return x\n", "E0218");
}

// Prelude (R35) and collections ----------------------------------------------------------

#[test]
fn the_prelude_needs_no_import() {
    clean(
        "fn f(xs: [int], s: str) -> int:\n    total = sum(xs) + len(s) + max(xs) + abs(min(xs))\n    ys = sorted(xs, key=lambda x: -x)\n    for i, x in enumerate(reversed(ys)):\n        print(f\"{i}: {x}\")\n    pairs = list(zip(xs, ys))\n    return total + len(pairs) + ord(\"a\") + len(chr(65)) + round(2.5)\n",
    );
}

#[test]
fn methods_of_builtin_types() {
    clean(
        "fn f(s: str, xs: [int], d: {str: int}) -> int:\n    parts = s.strip().lower().split(\",\")\n    n = s.to_int() ?? 0\n    at = s.find(\"x\") ?? -1\n    v = d.get(\"k\") ?? d.get(\"k\", 0)\n    last = xs.last() ?? 0\n    return len(parts) + n + at + v + last + len(d.keys())\n",
    );
}

#[test]
fn an_unknown_method_lists_the_type_s_methods() {
    let found = check_source("fn f(s: str) -> str:\n    return s.uppercase()\n");
    assert_eq!(found.iter().map(|d| d.code).collect::<Vec<_>>(), vec!["E0205"]);
    assert!(found[0].alternatives.contains(&"upper".to_string()));
}

#[test]
fn comprehensions_tuples_sets_and_slices() {
    clean(
        "fn f(xs: [int]) -> int:\n    squares = [x * x for x in xs if x > 1]\n    seen = {x % 3 for x in xs}\n    index = {str(x): x for x in xs}\n    pair = (1, \"a\")\n    a, b = pair\n    tail = xs[1:]\n    rev = xs[::-1]\n    return len(squares) + len(seen) + len(index) + a + len(b) + len(tail) + len(rev)\n",
    );
}

#[test]
fn imports_from_math() {
    clean(
        "from math import sqrt, pi\nimport math\n\nfn f(x: f64) -> f64:\n    return sqrt(x) * pi + float(math.floor(x))\n",
    );
    only("from os import getcwd\n\nfn f() -> int:\n    return 1\n", "E0216");
}

#[test]
fn test_blocks_are_checked() {
    only("test \"t\":\n    assert undefined_name == 1\n", "E0201");
    clean("fn f() -> int:\n    return 1\n\ntest \"t\":\n    assert f() == 1\n");
}

#[test]
fn a_name_declared_twice_is_reported() {
    only("fn f() -> int:\n    return 1\n\nfn f() -> int:\n    return 2\n", "E0210");
}

#[test]
fn calling_a_value_that_is_not_a_function_is_reported() {
    only("fn f(x: int) -> int:\n    return x(1)\n", "E0212");
}

#[test]
fn errors_do_not_cascade_from_an_unknown_name() {
    only("fn f() -> int:\n    y = unknown + 1\n    z = y * 2\n    return z.foo\n", "E0201");
}

#[test]
fn a_type_doubled_line_after_line_is_reported_once_it_is_too_large() {
    // Eighteen lines are enough to pass the limit at the sixteenth and few enough that, were it
    // gone, the checker would still finish in a few hundred megabytes rather than exhaust memory.
    let mut doubled = String::from("fn main():\n    t0 = 1\n");
    for i in 1..=18 {
        doubled += &format!("    t{i} = (t{}, t{})\n", i - 1, i - 1);
    }
    doubled += "    print(t18)\n";
    only(&doubled, "E0222");
    let items: Vec<String> = (0..1000).map(|i| i.to_string()).collect();
    clean(&format!("fn main():\n    t = ({})\n    print(t)\n", items.join(", ")));
}

#[test]
fn the_corpus_programs_check_clean() {
    // The paired corpus is the language's reference material; it must type-check. It is
    // frozen as the pilot's models saw it, so warnings about semantics settled since are allowed.
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../research/tokens/corpus");
    let entries = std::fs::read_dir(&corpus).expect("the corpus, from the repository root");
    for entry in entries.flatten() {
        let path = entry.path().join("b.x");
        if let Ok(text) = std::fs::read_to_string(&path) {
            let found: Vec<_> =
                check_source(&text).into_iter().filter(|d| d.severity == lotml_diag::Severity::Error).collect();
            assert!(
                found.is_empty(),
                "{}: {:#?}",
                path.display(),
                found.iter().map(|d| (d.code, &d.message)).collect::<Vec<_>>()
            );
        }
    }
}
