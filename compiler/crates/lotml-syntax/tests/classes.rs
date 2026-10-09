//! A Python class in an interface (specs/python-classes R1.5, adr:0034): a `class` block of
//! attribute lines and bodyless signatures, which an interface holds and a program never does.

use lotml_syntax::ast::Item;
use lotml_syntax::{parse, parse_interface};

const DATE: &str = "\
class date:
    year: int
    month: int
    fn date(year: int, month: int, day: int) -> date ! PyError
    fn today() -> date ! PyError
    fn isoformat(self) -> str ! PyError

class datetime(date):
    fn now() -> datetime ! PyError
";

#[test]
fn an_interface_reads_a_class_s_bases_attributes_and_signatures() {
    let parsed = parse_interface(DATE);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let [Item::Class(date), Item::Class(datetime)] = parsed.module.items.as_slice() else {
        panic!("two classes: {:?}", parsed.module.items);
    };
    assert_eq!(date.name.name, "date");
    assert!(date.bases.is_empty());
    let attributes: Vec<&str> =
        date.attributes.iter().filter_map(|a| a.name.as_ref()).map(|n| n.name.as_str()).collect();
    assert_eq!(attributes, ["year", "month"]);
    let methods: Vec<&str> = date.methods.iter().map(|m| m.name.name.as_str()).collect();
    assert_eq!(methods, ["date", "today", "isoformat"]);
    assert!(date.methods.iter().all(|m| m.body.is_none()), "signatures, no bodies");
    let bases: Vec<&str> = datetime.bases.iter().map(|b| b.name.as_str()).collect();
    assert_eq!(bases, ["date"]);
}

#[test]
fn a_program_still_refuses_class_with_e0102() {
    let parsed = parse("class date:\n    year: int\n");
    assert!(parsed.errors.iter().any(|e| e.code == "E0102"), "{:?}", parsed.errors);
    assert!(!parsed.module.items.iter().any(|i| matches!(i, Item::Class(_))));
}

#[test]
fn a_class_holds_attributes_and_signatures_and_nothing_else() {
    let parsed = parse_interface("class date:\n    x = 1\n    fn isoformat(self) -> str ! PyError\n");
    assert!(
        parsed.errors.iter().any(|e| e.message.contains("expected an attribute (`name: T`) or a method signature")),
        "{:?}",
        parsed.errors
    );
    let [Item::Class(date)] = parsed.module.items.as_slice() else { panic!() };
    assert_eq!(date.methods.len(), 1, "the line after the error is still read");
}

#[test]
fn a_class_with_no_member_is_its_header_alone() {
    let parsed = parse_interface("class empty\nclass child(empty)\nfn f() -> child ! PyError\n");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let [Item::Class(empty), Item::Class(child), Item::Fn(_)] = parsed.module.items.as_slice() else {
        panic!("{:?}", parsed.module.items);
    };
    assert!(empty.attributes.is_empty() && empty.methods.is_empty());
    assert_eq!(child.bases[0].name, "empty");
}

#[test]
fn a_generic_class_reads_its_type_parameters_and_a_method_its_typed_self() {
    let text = "class Pattern[AnyStr, T](Base):\n    pattern: AnyStr\n    fn search(self: Pattern[str], s: str) -> str ! PyError\n\nclass Box[T]\n";
    let parsed = parse_interface(text);
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let [Item::Class(pattern), Item::Class(boxed)] = parsed.module.items.as_slice() else { panic!() };
    let params: Vec<&str> = pattern.type_params.iter().map(|p| p.name.name.as_str()).collect();
    assert_eq!(params, ["AnyStr", "T"]);
    assert_eq!(pattern.bases[0].name, "Base");
    assert!(pattern.methods[0].params[0].ty.is_some(), "`self` keeps its annotation");
    assert_eq!(boxed.type_params[0].name.name, "T");
}
