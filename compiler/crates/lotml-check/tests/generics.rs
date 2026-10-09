//! Python generics (specs/python-generics, adr:0036): a stub's type variables crossed as type
//! parameters of the functions, classes and members a Python interface declares.

use lotml_check::interface_of;
use lotml_check::ty::Ty;

const RE: &str = "\
fn nlargest[T](n: int, iterable: [T]) -> [T] ! PyError
fn compile(pattern: str) -> Pattern[str] ! PyError

class Pattern[AnyStr]:
    pattern: AnyStr
    fn Pattern(pattern: AnyStr) -> Pattern[AnyStr] ! PyError
    fn escape(text: AnyStr) -> AnyStr ! PyError
    fn search(self: Pattern[str], string: str) -> str ! PyError
    fn split(self, string: AnyStr) -> [AnyStr] ! PyError
    fn mapped[S](self, value: S) -> [S] ! PyError
";

fn messages(problems: &[lotml_diag::Diagnostic]) -> Vec<&str> {
    problems.iter().map(|d| d.message.as_str()).collect()
}

fn param(name: &str) -> Ty {
    Ty::Param(name.into())
}

fn pattern(arg: Ty) -> Ty {
    Ty::Adt("py.re.Pattern".into(), vec![arg])
}

#[test]
fn a_generic_function_keeps_its_type_parameters_and_is_marked_python() {
    let (read, problems) = interface_of("py.re", RE);
    assert!(problems.is_empty(), "{:?}", messages(&problems));
    let nlargest = read.function("nlargest").unwrap();
    assert_eq!(nlargest.type_params, [("T".to_string(), None)]);
    assert_eq!(nlargest.ret, Ty::list(param("T")));
    assert!(nlargest.python, "a Python signature is marked as one");
    assert_eq!(read.function("compile").unwrap().ret, pattern(Ty::Str), "a class named with its arguments");
}

#[test]
fn a_generic_class_s_members_are_typed_by_its_parameters() {
    let (read, _) = interface_of("py.re", RE);
    let (_, class) = read.classes().next().unwrap();
    assert_eq!(class.params, ["AnyStr"]);
    assert_eq!(class.attributes["pattern"], param("AnyStr"));
    let constructor = class.constructor.as_ref().unwrap();
    assert_eq!(constructor.type_params, [("AnyStr".to_string(), None)], "a constructor infers the class's");
    assert_eq!(constructor.ret, pattern(param("AnyStr")));
    let escape = &class.methods["escape"];
    assert!(escape.receiver.is_none() && escape.owner_params.is_empty());
    assert_eq!(escape.sig.type_params, [("AnyStr".to_string(), None)], "so does a static method");
    let split = &class.methods["split"];
    assert_eq!(split.owner_params, ["AnyStr"], "a method takes the receiver's arguments");
    assert_eq!(split.sig.params[0].ty, pattern(param("AnyStr")));
    assert_eq!(class.methods["search"].sig.params[0].ty, pattern(Ty::Str), "an annotated `self` is kept");
    let mapped = &class.methods["mapped"];
    assert_eq!(
        (mapped.owner_params.as_slice(), mapped.sig.type_params.as_slice()),
        (&["AnyStr".to_string()][..], &[("S".to_string(), None)][..])
    );
}

#[test]
fn a_bounded_type_parameter_is_refused() {
    for (text, says) in [
        ("fn f[T: Ord](x: T) -> T ! PyError\n", "`T` of `f` has a bound"),
        ("class Box[T: Ord]\n", "`T` of `Box` has a bound"),
    ] {
        let (_, problems) = interface_of("py.m", text);
        assert!(
            problems.iter().any(|d| d.code == "E0221" && d.message.contains(says)),
            "{text}: {:?}",
            messages(&problems)
        );
    }
}

#[test]
fn a_generic_base_s_constructor_is_not_inherited() {
    let text = "class Base[T]:\n    fn Base(x: T) -> Base[T] ! PyError\n\nclass Plain:\n    fn Plain(n: int) -> Plain ! PyError\n\nclass Sub(Base)\nclass Leaf(Plain)\n";
    let (read, problems) = interface_of("py.m", text);
    assert!(problems.is_empty(), "{:?}", messages(&problems));
    let classes: std::collections::BTreeMap<_, _> = read.classes().collect();
    let constructors = |c: &str| classes.get(c.strip_prefix("py.m.")?)?.constructor.as_ref();
    let bases = |c: &str| classes.get(c.strip_prefix("py.m.")?).map(|class| class.bases.as_slice());
    assert!(lotml_check::py_constructor("py.m.Sub", constructors, bases).is_none());
    let leaf = lotml_check::py_constructor("py.m.Leaf", constructors, bases).unwrap();
    assert_eq!(leaf.ret, Ty::Adt("py.m.Leaf".into(), vec![]));
    let own = lotml_check::py_constructor("py.m.Base", constructors, bases).unwrap();
    assert_eq!(own.ret, Ty::Adt("py.m.Base".into(), vec![param("T")]), "its own keeps its parameters");
}

fn check(source: &str) -> lotml_check::Checked {
    let (read, problems) = interface_of("py.re", RE);
    assert!(problems.is_empty());
    let interfaces = lotml_check::Interfaces::from([("py.re".to_string(), read)]);
    let parsed = lotml_syntax::parse(source);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    lotml_check::check_resolved_with(&parsed.module, source, &interfaces)
}

fn codes(checked: &lotml_check::Checked) -> Vec<String> {
    checked.diagnostics.iter().map(|d| format!("{} {}", d.code, d.message)).collect()
}

/// The signature recorded for the call written `call` in `source`.
fn recorded(source: &str, call: &str) -> lotml_check::FnSig {
    let checked = check(source);
    assert!(checked.diagnostics.is_empty(), "{source}\n{:?}", codes(&checked));
    let start = source.find(call).unwrap() as u32;
    let at = lotml_syntax::span::Span { start, end: start + call.len() as u32 };
    checked.py_calls.get(&at).unwrap_or_else(|| panic!("nothing recorded at `{call}`")).sig.clone()
}

#[test]
fn a_generic_python_call_infers_its_type_arguments_and_records_them() {
    let source = "from py.re import nlargest\n\nfn f() -> [int] ! PyError:\n    return nlargest(2, [3, 1, 2])?\n";
    let sig = recorded(source, "nlargest(2, [3, 1, 2])");
    let int = Ty::primitive("int").unwrap();
    assert_eq!(sig.params[1].ty, Ty::list(int.clone()), "the parameter as instantiated");
    assert_eq!(sig.ret, Ty::list(int));
    let plain = "from py.re import compile\n\nfn f() -> None ! PyError:\n    p = compile(\"a\")?\n";
    assert_eq!(recorded(plain, "compile(\"a\")").ret, pattern(Ty::Str), "every Python call is recorded");
}

#[test]
fn a_type_argument_the_boundary_does_not_carry_is_refused() {
    let source =
        "from py.re import nlargest\n\ntype P(x: int)\n\nfn f() -> None ! PyError:\n    top = nlargest(1, [P(1)])?\n";
    let found = codes(&check(source));
    assert!(
        found.len() == 1 && found[0].starts_with("E0204") && found[0].contains("`P`"),
        "a record does not cross into Python: {found:?}"
    );
    let handle =
        "from py.re import nlargest, compile\n\nfn f() -> None ! PyError:\n    top = nlargest(1, [compile(\"a\")?])?\n";
    assert!(codes(&check(handle)).is_empty(), "a Python class's value does: {:?}", codes(&check(handle)));
}
