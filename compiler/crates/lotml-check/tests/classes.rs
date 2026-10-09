//! Python classes an interface declares (specs/python-classes, adr:0034): read with their bases,
//! attributes, constructor, methods and static methods, each type written by its module.

use lotml_check::ty::Ty;
use lotml_check::{c_interface, interface, interface_of};

const DATETIME: &str = "\
class date:
    year: int
    fn date(year: int, month: int, day: int) -> date ! PyError
    fn today() -> date ! PyError
    fn isoformat(self) -> str ! PyError

class datetime(date):
    fn now() -> datetime ! PyError

fn combine(d: date, hour: int) -> datetime ! PyError
";

fn class(name: &str) -> Ty {
    Ty::Adt(format!("py.datetime.{name}"), Vec::new())
}

#[test]
fn a_class_is_read_with_its_members_its_types_written_by_its_module() {
    let (read, problems) = interface_of("py.datetime", DATETIME);
    assert!(problems.is_empty(), "{:?}", problems.iter().map(|d| &d.message).collect::<Vec<_>>());
    let classes: Vec<(&str, &lotml_check::PyClass)> = read.classes().collect();
    let [("date", date), ("datetime", datetime)] = classes.as_slice() else { panic!("{classes:?}") };
    assert_eq!(date.attributes["year"], Ty::primitive("int").unwrap());
    let constructor = date.constructor.as_ref().expect("a constructor");
    assert_eq!(constructor.ret, class("date"));
    assert_eq!(constructor.params.len(), 3);
    assert!(date.methods["isoformat"].receiver.is_some(), "a method takes `self`");
    assert!(date.methods["today"].receiver.is_none(), "a static method does not");
    assert_eq!(date.methods["isoformat"].sig.params[0].ty, class("date"));
    assert_eq!(datetime.bases, ["py.datetime.date"]);
    let combine: Vec<&str> = read.names().collect();
    assert_eq!(combine, ["combine"]);
}

#[test]
fn a_function_of_the_interface_is_typed_by_its_classes() {
    let (read, _) = interface_of("py.datetime", DATETIME);
    let (_, problems) = interface(DATETIME);
    assert!(problems.is_empty(), "the same interface read with no module named");
    let classes: Vec<&str> = read.classes().map(|(name, _)| name).collect();
    assert_eq!(classes, ["date", "datetime"]);
    let (unnamed, _) = interface(DATETIME);
    let date = unnamed.classes().next().unwrap().1;
    assert_eq!(date.constructor.as_ref().unwrap().ret, Ty::Adt("py._.date".into(), Vec::new()));
}

#[test]
fn a_class_holds_only_what_python_can_check() {
    let cases = [
        ("class datetime(date):\n    fn now() -> datetime ! PyError\n", "no class this interface declares"),
        ("class date:\n    fn isoformat(self) -> str\n", "must return `T ! PyError`"),
        ("class date:\n    fn date(self, year: int) -> date ! PyError\n", "takes no `self`"),
        ("class date:\n    fn date(year: int) -> int ! PyError\n", "takes no `self` and returns `date`"),
        ("class date:\n    fn f[T](self, x: T) -> T ! PyError\n", "type parameters"),
        ("class date:\n    year: int\n\nclass date:\n    month: int\n", "declared twice"),
    ];
    for (text, says) in cases {
        let (_, problems) = interface_of("py.datetime", text);
        assert!(
            problems.iter().any(|d| d.message.contains(says)),
            "{text}: {:?}",
            problems.iter().map(|d| &d.message).collect::<Vec<_>>()
        );
    }
    let (_, problems) = c_interface("class point:\n    x: int\n");
    assert!(problems.iter().any(|d| d.code == "E0221"), "a C interface declares no class");
}

fn interfaces() -> lotml_check::Interfaces {
    let (read, problems) = interface_of("py.datetime", DATETIME);
    assert!(problems.is_empty());
    lotml_check::Interfaces::from([("py.datetime".to_string(), read)])
}

fn codes(source: &str) -> Vec<&'static str> {
    let parsed = lotml_syntax::parse(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    lotml_check::check_resolved_with(&parsed.module, source, &interfaces()).diagnostics.iter().map(|d| d.code).collect()
}

fn messages(source: &str) -> Vec<String> {
    let parsed = lotml_syntax::parse(source);
    lotml_check::check_resolved_with(&parsed.module, source, &interfaces())
        .diagnostics
        .iter()
        .map(|d| format!("{} {}", d.message, d.notes.join(" ")))
        .collect()
}

#[test]
fn an_imported_class_is_a_type_and_its_name_calls_its_constructor() {
    let source = "from py.datetime import date\n\nfn first() -> date ! PyError:\n    d: date = date(2026, 1, 8)?\n    return d\n";
    assert_eq!(codes(source), Vec::<&str>::new());
    let wrong = "from py.datetime import date\n\nfn first() -> date ! PyError:\n    return date(\"2026\", 1, 8)?\n";
    assert_eq!(codes(wrong), ["E0204"], "the constructor's parameters are checked");
    let unwrapped = "from py.datetime import date\n\nfn first() -> date:\n    return date(2026, 1, 8)\n";
    assert!(
        messages(unwrapped).iter().any(|m| m.contains("py.datetime.date ! PyError")),
        "a constructor can fail: {:?}",
        messages(unwrapped)
    );
}

#[test]
fn a_class_is_reached_by_its_module_s_path_too() {
    let source = "import py.datetime\n\nfn first() -> None ! PyError:\n    d = py.datetime.date(2026, 1, 8)?\n";
    assert_eq!(codes(source), Vec::<&str>::new());
}

#[test]
fn a_class_imported_beside_a_record_of_its_name_is_declared_twice() {
    let source = "from py.datetime import date\n\ntype date(year: int)\n";
    assert!(codes(source).contains(&"E0210"), "{:?}", messages(source));
}

const USES: &str = "from py.datetime import date, datetime, combine\n\n";

fn clean(body: &str) {
    let source = format!("{USES}{body}");
    assert_eq!(codes(&source), Vec::<&str>::new(), "{source}\n{:?}", messages(&source));
}

fn refused(body: &str, says: &str) {
    let source = format!("{USES}{body}");
    let found = messages(&source);
    assert!(found.iter().any(|m| m.contains(says)), "{source}\n{found:?}");
}

#[test]
fn a_method_a_static_method_and_an_attribute_are_typed_and_can_fail() {
    clean("fn f() -> str ! PyError:\n    d = date(2026, 1, 8)?\n    return d.isoformat()?\n");
    clean("fn f() -> date ! PyError:\n    return date.today()?\n");
    clean("fn f() -> int ! PyError:\n    d = date.today()?\n    return d.year?\n");
    refused("fn f() -> str ! PyError:\n    d = date.today()?\n    return d.isoformat()\n", "str ! PyError");
    refused("fn f() -> int ! PyError:\n    d = date.today()?\n    return d.year\n", "int ! PyError");
}

#[test]
fn a_subclass_reaches_the_members_of_the_bases_it_declares() {
    clean("fn f() -> str ! PyError:\n    now = datetime.now()?\n    return now.isoformat()?\n");
    clean("fn f() -> int ! PyError:\n    now = datetime.now()?\n    return now.year?\n");
    clean("fn f() -> date ! PyError:\n    return datetime.today()?\n");
}

#[test]
fn a_member_is_used_as_the_interface_declares_it() {
    refused("fn f() -> None ! PyError:\n    d = date.today()?\n    d.nope()?\n", "has no method `nope`");
    refused("fn f() -> None ! PyError:\n    d = date.today()?\n    x = d.year()?\n", "is an attribute");
    refused("fn f() -> None ! PyError:\n    d = date.today()?\n    x = d.isoformat?\n", "is a method");
    refused("fn f() -> None ! PyError:\n    d = date.today()?\n    x = d.today()?\n", "takes no `self`");
    refused("fn f() -> None ! PyError:\n    x = date.isoformat()?\n", "is a method: call it on a `date` value");
    refused("fn f() -> None ! PyError:\n    x = date.nope()?\n", "has no function `nope`");
}

#[test]
fn an_attribute_of_a_python_object_is_never_assigned() {
    refused("fn f() -> None ! PyError:\n    var d = date.today()?\n    d.year = 2027\n", "read, never assigned");
}

#[test]
fn a_subclass_goes_where_its_base_is_expected_and_nowhere_else() {
    clean("fn f() -> date ! PyError:\n    return datetime.now()?\n");
    clean("fn f() -> datetime ! PyError:\n    now = datetime.now()?\n    return combine(now, 3)?\n");
    refused(
        "fn f() -> datetime ! PyError:\n    return date.today()?\n",
        "expected `py.datetime.datetime`, found `py.datetime.date`",
    );
    refused("fn f() -> str ! PyError:\n    return date.today()?\n", "expected `str`");
}

#[test]
fn a_python_class_goes_where_a_python_object_is_expected() {
    let (read, _) = interface_of("py.datetime", &format!("{DATETIME}fn keep(o: PyObject) -> None ! PyError\n"));
    let interfaces = lotml_check::Interfaces::from([("py.datetime".to_string(), read)]);
    let source = "from py.datetime import date, keep\n\nfn f() -> None ! PyError:\n    keep(date.today()?)?\n";
    let parsed = lotml_syntax::parse(source);
    let found = lotml_check::check_resolved_with(&parsed.module, source, &interfaces).diagnostics;
    assert!(found.is_empty(), "{:?}", found.iter().map(|d| &d.message).collect::<Vec<_>>());
}

#[test]
fn a_python_class_s_value_has_only_its_declared_members() {
    let cases = [
        "    x = d + 1\n",
        "    same = d == d\n",
        "    for part in d:\n        pass\n",
        "    n = len(d)\n",
        "    print(d)\n",
        "    print(f\"{d}\")\n",
        "    s = str(d)\n",
        "    seen = {d}\n",
        "    index = {d: 1}\n",
        "    print([d])\n",
    ];
    for case in cases {
        let source = format!("{USES}fn f() -> None ! PyError:\n    d = date.today()?\n{case}");
        let parsed = lotml_syntax::parse(&source);
        let found = lotml_check::check_resolved_with(&parsed.module, &source, &interfaces()).diagnostics;
        assert!(
            found
                .iter()
                .any(|d| d.code == "E0204" && d.notes.iter().any(|n| n.contains("only the methods and attributes"))),
            "{case}: {:?}",
            found.iter().map(|d| (&d.message, &d.notes)).collect::<Vec<_>>()
        );
    }
}

#[test]
fn a_class_without_a_constructor_of_its_own_is_built_through_its_base_s() {
    clean("fn f() -> datetime ! PyError:\n    return datetime(2026, 1, 8)?\n");
    refused("fn f() -> datetime ! PyError:\n    return datetime(\"2026\", 1, 8)?\n", "expected `int`");
    let typed = "import py.datetime\n\nfn f() -> None ! PyError:\n    now = py.datetime.datetime(2026, 1, 8)?\n    print(now.isoformat()?)\n";
    let found = lotml_check::check_resolved_with(&lotml_syntax::parse(typed).module, typed, &interfaces()).diagnostics;
    assert!(
        found.is_empty(),
        "through the module's path too: {:?}",
        found.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}

#[test]
fn a_long_chain_of_classes_and_a_cycle_of_bases_are_checked_in_time() {
    let mut text = String::from("class c0:\n    fn c0(n: int) -> c0 ! PyError\n    fn f(self) -> int ! PyError\n");
    for i in 1..3000 {
        text += &format!("class c{i}(c{})\n", i - 1);
    }
    text += "class a(b)\nclass b(a)\n";
    let (read, problems) = interface_of("py.chain", &text);
    assert!(problems.is_empty(), "{:?}", problems.iter().take(3).map(|d| &d.message).collect::<Vec<_>>());
    let interfaces = lotml_check::Interfaces::from([("py.chain".to_string(), read)]);
    let source = "from py.chain import c2999, a\n\nfn f(x: a) -> int ! PyError:\n    c = c2999(1)?\n    x.f()?\n    return c.f()?\n";
    let started = std::time::Instant::now();
    let parsed = lotml_syntax::parse(source);
    let found = lotml_check::check_resolved_with(&parsed.module, source, &interfaces).diagnostics;
    assert!(started.elapsed() < std::time::Duration::from_secs(5), "{:?}", started.elapsed());
    let codes: Vec<&str> = found.iter().map(|d| d.code).collect();
    assert_eq!(
        codes,
        ["E0205"],
        "the chain's method found, the cycle's ended: {:?}",
        found.iter().map(|d| &d.message).collect::<Vec<_>>()
    );
}
