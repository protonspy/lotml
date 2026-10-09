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
