//! The opaque Python value (specs/python-object, adr:0031): `PyObject` is a type of programs and
//! interfaces, takes any value the boundary carries where Python expects one, has no operation of
//! its own, and gives a LotML value only through `o.value()` typed from its context.

use lotml_check::{Interfaces, check_resolved_with, interface};
use lotml_diag::Diagnostic;
use lotml_syntax::parse;

const REQUESTS: &str = "\
fn get(url: str) -> PyObject ! PyError
fn status(response: PyObject) -> int ! PyError
fn send(payload: PyObject) -> None ! PyError
fn many(items: [PyObject], table: {str: PyObject}?) -> int ! PyError
";

fn diagnostics(body: &str) -> Vec<Diagnostic> {
    let (requests, problems) = interface(REQUESTS);
    assert!(problems.is_empty(), "{:?}", problems.iter().map(|d| &d.message).collect::<Vec<_>>());
    let interfaces = Interfaces::from([("py.requests".to_string(), requests)]);
    let source = format!(
        "import py.requests\n\ntype Point(x: int)\n\nfn f() -> None ! PyError:\n    r = py.requests.get(\"u\")?\n{body}"
    );
    check_resolved_with(&parse(&source).module, &source, &interfaces).diagnostics
}

fn clean(body: &str) {
    let found = diagnostics(body);
    assert!(found.is_empty(), "{body}\n{:?}", found.iter().map(|d| (d.code, &d.message)).collect::<Vec<_>>());
}

/// The one diagnostic `body` gets, which says to convert the value.
fn refused(body: &str) -> &'static str {
    let found = diagnostics(body);
    assert_eq!(found.len(), 1, "{body}\n{:?}", found.iter().map(|d| (d.code, &d.message)).collect::<Vec<_>>());
    assert!(found[0].notes.iter().any(|n| n.contains("o.value()")), "{body}: {:?}", found[0].notes);
    found[0].code
}

#[test]
fn a_python_object_is_stored_passed_and_returned() {
    clean(
        "    code = py.requests.status(r)?\n    print(code)\n    py.requests.send(r)?\n    kept = [r, r]\n    py.requests.send(kept[0])?\n",
    );
}

#[test]
fn where_python_expects_one_any_value_the_boundary_carries_is_taken() {
    clean("    py.requests.send(1)?\n    py.requests.send(\"text\")?\n    py.requests.send([1, 2])?\n");
    clean("    py.requests.send({\"a\": [1.5]})?\n    py.requests.send((1, \"x\"))?\n    py.requests.send(None)?\n");
    let found = diagnostics("    py.requests.send(Point(1))?\n");
    assert_eq!(found.len(), 1, "a record is not what the boundary carries into Python");
    clean("    n = py.requests.many([1, 2], {\"a\": [1.5]})?\n    m = py.requests.many([r, r], None)?\n");
    let found = diagnostics("    n = py.requests.many([Point(1)], None)?\n");
    assert_eq!(found.len(), 1, "part by part, a record is still not carried");
}

#[test]
fn every_operation_on_a_python_object_is_refused_with_the_way_out() {
    assert_eq!(refused("    print(r.status_code)\n"), "E0205");
    assert_eq!(refused("    data = r.json()\n"), "E0205");
    assert_eq!(refused("    x = r + 1\n"), "E0204");
    assert_eq!(refused("    same = r == r\n"), "E0204");
    assert_eq!(refused("    for line in r:\n        pass\n"), "E0204");
    assert_eq!(refused("    print(r)\n"), "E0204");
    assert_eq!(refused("    print(f\"{r}\")\n"), "E0204");
    assert_eq!(refused("    n = len(r)\n"), "E0204");
    assert_eq!(refused("    s = str(r)\n"), "E0204");
}

#[test]
fn value_takes_its_type_from_its_context() {
    clean("    n: int = r.value()?\n    print(n + 1)\n");
    clean("    names: [str] = r.value()?\n    print(len(names))\n");
    clean("    table: {str: [int]}? = r.value()?\n    print(table is None)\n");
    let source = "fn g(r: PyObject) -> [str] ! PyError:\n    return r.value()?\n";
    let found = check_resolved_with(&parse(source).module, source, &Interfaces::new()).diagnostics;
    assert!(found.is_empty(), "the return type names it: {:?}", found.iter().map(|d| &d.message).collect::<Vec<_>>());
}

#[test]
fn a_value_is_a_result_that_can_fail() {
    let found = diagnostics("    n: int = r.value()\n");
    assert_eq!(found.iter().map(|d| d.code).collect::<Vec<_>>(), vec!["E0219"], "unwrapped, it is still a result");
}

#[test]
fn value_with_no_type_to_take_out_is_refused() {
    let found = diagnostics("    x = r.value()?\n");
    assert_eq!(found[0].code, "E0205");
    assert!(found[0].notes.iter().any(|n| n.contains("annotate")), "{:?}", found[0].notes);
    let found = diagnostics("    p: Point = r.value()?\n");
    assert_eq!(found[0].code, "E0204", "a record is not what the boundary carries: {:?}", found[0].message);
}
