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

fn checked(body: &str) -> (String, lotml_check::Checked) {
    let source = format!("from py.os import listdir, Path\n\n{body}");
    let parsed = lotml_syntax::parse(&source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let (read, problems) = interface_of("py.os", OS);
    assert!(problems.is_empty());
    let interfaces = lotml_check::Interfaces::from([("py.os".to_string(), read)]);
    let checked = lotml_check::check_resolved_with(&parsed.module, &source, &interfaces);
    (source, checked)
}

/// The overload the checker gave the call written `call` in `body`, which checks clean.
fn chosen(body: &str, call: &str) -> usize {
    let (source, checked) = checked(body);
    let found: Vec<String> =
        checked.diagnostics.iter().map(|d| format!("{} {} {}", d.code, d.message, d.notes.join(" "))).collect();
    assert!(found.is_empty(), "{source}\n{found:?}");
    let start = source.find(call).unwrap() as u32;
    let at = lotml_syntax::span::Span { start, end: start + call.len() as u32 };
    *checked
        .py_overloads
        .get(&at)
        .unwrap_or_else(|| panic!("no overload recorded at `{call}`: {:?}", checked.py_overloads))
}

fn refused(body: &str) -> Vec<String> {
    let (_, checked) = checked(body);
    checked.diagnostics.iter().map(|d| format!("{} {} {}", d.code, d.message, d.notes.join(" "))).collect()
}

#[test]
fn a_call_takes_the_first_overload_its_arguments_fit_and_its_result() {
    assert_eq!(chosen("fn f() -> [str] ! PyError:\n    return listdir(\".\")?\n", "listdir(\".\")"), 0);
    assert_eq!(chosen("fn f() -> [str] ! PyError:\n    return listdir()?\n", "listdir()"), 0, "a default fills");
    assert_eq!(chosen("fn f(raw: bytes) -> [bytes] ! PyError:\n    return listdir(raw)?\n", "listdir(raw)"), 1);
    assert_eq!(chosen("fn f() -> [str] ! PyError:\n    return listdir(path=3)?\n", "listdir(path=3)"), 2, "by keyword");
}

#[test]
fn a_value_given_to_a_py_object_parameter_is_the_fit_taken_last() {
    assert_eq!(
        chosen("fn f() -> [str] ! PyError:\n    return listdir(3)?\n", "listdir(3)"),
        2,
        "`int` fits the third exactly, the second only as a `PyObject`"
    );
    assert_eq!(
        chosen("fn f() -> [bytes] ! PyError:\n    return listdir(2.5)?\n", "listdir(2.5)"),
        1,
        "with no exact fit, the first that takes it as a `PyObject`"
    );
}

#[test]
fn a_constructor_a_method_and_through_the_module_path_choose_too() {
    assert_eq!(chosen("fn f() -> Path ! PyError:\n    return Path([\"a\", \"b\"])?\n", "Path([\"a\", \"b\"])"), 1);
    let body = "fn f() -> Path ! PyError:\n    p = Path(\"a\")?\n    return p.joined(p)?\n";
    assert_eq!(chosen(body, "p.joined(p)"), 1);
    assert_eq!(chosen(body, "Path(\"a\")"), 0);
    let module = "import py.os\n\nfn f() -> [str] ! PyError:\n    return py.os.listdir(3)?\n";
    assert_eq!(chosen(module, "py.os.listdir(3)"), 2);
}

#[test]
fn a_call_no_overload_takes_lists_them() {
    let found = refused("fn f() -> None ! PyError:\n    p = Path(\"a\")?\n    q = p.joined(3)?\n");
    assert_eq!(found.len(), 1, "{found:?}");
    assert!(found[0].starts_with("E0204"), "{found:?}");
    assert!(
        found[0].contains("other: str") && found[0].contains("other: py.os.Path"),
        "each overload listed: {found:?}"
    );
    let found = refused("fn f() -> None ! PyError:\n    xs = listdir(\".\", 2)?\n");
    assert!(found.len() == 1 && found[0].starts_with("E0204"), "too many arguments for any: {found:?}");
}

#[test]
fn a_nest_of_overloaded_calls_checks_each_argument_once() {
    let nest: String = (0..30).fold("p".to_string(), |inner, _| format!("p.joined({inner})?"));
    let started = std::time::Instant::now();
    let body = format!("fn f() -> Path ! PyError:\n    p = Path(\"a\")?\n    return {nest}\n");
    let found = refused(&body);
    assert!(found.is_empty(), "{found:?}");
    assert!(started.elapsed().as_secs() < 5, "thirty nested overloaded calls took {:?}", started.elapsed());
}

#[test]
fn an_overloaded_function_is_called_never_passed() {
    for body in ["fn f() -> None:\n    g = listdir\n", "import py.os\n\nfn f() -> None:\n    g = py.os.listdir\n"] {
        let found = refused(body);
        assert!(found.len() == 1 && found[0].starts_with("E0226"), "{body}\n{found:?}");
    }
    assert!(refused("fn f() -> None:\n    g = lambda path: listdir(path)\n").is_empty(), "a lambda calls it");
    let (read, _) = interface_of("py.m", "fn one(x: int) -> int ! PyError\n");
    let interfaces = lotml_check::Interfaces::from([("py.m".to_string(), read)]);
    let source = "from py.m import one\n\nfn f() -> None:\n    g = one\n";
    let parsed = lotml_syntax::parse(source);
    let checked = lotml_check::check_resolved_with(&parsed.module, source, &interfaces);
    assert!(checked.diagnostics.is_empty(), "a function of one signature is still a value");
}

#[test]
fn a_long_lineage_is_walked_once_however_many_overloads_a_call_tries() {
    let mut text = String::from("class C0\n");
    for k in 1..3000 {
        text.push_str(&format!("class C{k}(C{})\n", k - 1));
    }
    for k in 0..OVERLOADS - 1 {
        text.push_str(&format!("class O{k}\nfn f(x: O{k}) -> int ! PyError\n"));
    }
    text.push_str("fn f(x: C0) -> int ! PyError\nfn make() -> C2999 ! PyError\n");
    let (read, problems) = interface_of("py.m", &text);
    assert!(problems.is_empty(), "{:?}", messages(&problems));
    let interfaces = lotml_check::Interfaces::from([("py.m".to_string(), read)]);
    let calls: String = (0..300).map(|i| format!("    n{i} = f(c)?\n")).collect();
    let source = format!("from py.m import f, make\n\nfn g() -> None ! PyError:\n    c = make()?\n{calls}");
    let parsed = lotml_syntax::parse(&source);
    let started = std::time::Instant::now();
    let checked = lotml_check::check_resolved_with(&parsed.module, &source, &interfaces);
    assert!(started.elapsed().as_secs() < 5, "took {:?}", started.elapsed());
    let found: Vec<&str> = checked.diagnostics.iter().map(|d| d.code).collect();
    assert!(found.is_empty(), "the last overload, of `C0`, takes a `C2999`: {found:?}");
}
