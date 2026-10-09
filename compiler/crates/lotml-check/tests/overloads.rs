//! Python overloads (specs/python-overloads, adr:0035): a function, constructor or method a Python
//! interface declares more than once is one with those overloads, in order.

use lotml_check::ty::Ty;
use lotml_check::{OVERLOADS, c_interface, interface_of};

const OS: &str = "\
fn listdir(path: str? = todo()) -> [str] ! PyError
fn listdir(path: PyObject) -> [bytes] ! PyError
fn listdir(path: int) -> [str] ! PyError

class Path:
    fn Path(text: str) -> Path ! PyError
    fn Path(parts: [str]) -> Path ! PyError
    fn joined(self, other: str) -> Path ! PyError
    fn joined(self, other: Path) -> Path ! PyError
    fn cwd() -> Path ! PyError

class PosixPath(Path)
";

fn messages(problems: &[lotml_diag::Diagnostic]) -> Vec<&str> {
    problems.iter().map(|d| d.message.as_str()).collect()
}

#[test]
fn a_function_declared_again_is_one_with_its_overloads_in_order() {
    let (read, problems) = interface_of("py.os", OS);
    assert!(problems.is_empty(), "{:?}", messages(&problems));
    assert_eq!(read.names().collect::<Vec<_>>(), ["listdir"], "one function, not three");
    let listdir = read.function("listdir").unwrap();
    let returns: Vec<Ty> =
        std::iter::once(listdir).chain(&listdir.overloads).map(|sig| sig.params[0].ty.clone()).collect();
    assert_eq!(returns, [Ty::Optional(Box::new(Ty::Str)), Ty::PyObject, Ty::primitive("int").unwrap()]);
    assert!(listdir.overloads.iter().all(|o| o.overloads.is_empty()), "an overload holds none of its own");
}

#[test]
fn a_constructor_and_a_method_declared_again_are_ones_with_overloads() {
    let (read, _) = interface_of("py.os", OS);
    let classes: Vec<_> = read.classes().collect();
    let path = classes.iter().find(|(name, _)| *name == "Path").unwrap().1;
    let constructor = path.constructor.as_ref().unwrap();
    assert_eq!(constructor.overloads.len(), 1);
    assert_eq!(constructor.overloads[0].params[0].ty, Ty::list(Ty::Str));
    assert_eq!(path.methods["joined"].sig.overloads.len(), 1);
    assert_eq!(path.methods["joined"].sig.overloads[0].params[1].ty, Ty::Adt("py.os.Path".into(), vec![]));
    assert!(path.methods["cwd"].sig.overloads.is_empty());
}

#[test]
fn overloads_that_differ_in_taking_self_keep_the_first_and_say_why() {
    let text = "class C:\n    fn m(self) -> int ! PyError\n    fn m() -> str ! PyError\n    fn m(self, x: int) -> int ! PyError\n";
    let (read, problems) = interface_of("py.m", text);
    assert_eq!(problems.len(), 1, "{:?}", messages(&problems));
    assert_eq!(problems[0].code, "E0221");
    assert!(problems[0].message.contains("takes `self` where its first does not"));
    let class = read.classes().next().unwrap().1;
    assert!(class.methods["m"].sig.overloads.is_empty(), "the overloads after the refused one are left out too");
}

#[test]
fn a_name_keeps_at_most_its_limit_of_overloads() {
    let text: String = (0..OVERLOADS + 5).map(|i| format!("fn f(x: int, n{i}: int) -> int ! PyError\n")).collect();
    let (read, problems) = interface_of("py.m", &text);
    assert_eq!(problems.len(), 1, "one report, at the first past the limit: {:?}", messages(&problems));
    assert_eq!(problems[0].code, "E0221");
    assert_eq!(read.function("f").unwrap().overloads.len() + 1, OVERLOADS);
}

#[test]
fn a_c_library_and_a_class_named_as_a_function_still_declare_a_name_once() {
    let (_, problems) = c_interface("fn f(x: int) -> int\nfn f(x: f64) -> int\n");
    assert!(problems.iter().any(|d| d.code == "E0210"), "{:?}", messages(&problems));
    let (_, problems) = interface_of("py.m", "class f:\n    x: int\n\nfn f() -> int ! PyError\n");
    assert!(problems.iter().any(|d| d.code == "E0210"), "{:?}", messages(&problems));
}

#[test]
fn a_constructor_a_subclass_inherits_returns_the_subclass_from_every_overload() {
    let (read, _) = interface_of("py.os", OS);
    let classes: std::collections::BTreeMap<_, _> = read.classes().collect();
    let constructors = |c: &str| classes.get(c.strip_prefix("py.os.")?)?.constructor.as_ref();
    let bases = |c: &str| classes.get(c.strip_prefix("py.os.")?).map(|class| class.bases.as_slice());
    let inherited = lotml_check::py_constructor("py.os.PosixPath", constructors, bases).unwrap();
    let posix = Ty::Adt("py.os.PosixPath".into(), vec![]);
    assert_eq!(inherited.ret, posix);
    assert_eq!(inherited.overloads[0].ret, posix);
    assert_eq!(inherited.overloads[0].name, "PosixPath");
}
