//! Dicts and sets on the C target: insertion order for dicts, CPython's hash and table order for
//! sets, under `PYTHONHASHSEED=0` (R1.2, R1.3). A set display of three or more constants iterates
//! as CPython 3.12 and 3.13 build it, the versions CI and the harness run; run these tests with
//! `LOTML_PYTHON` naming one of them.

mod common;

use common::{parity, run_c_with};

const DICTS: &str = include_str!("programs/dicts.lotml");

#[test]
fn dicts_and_sets_print_and_iterate_in_python_s_order() {
    let run = parity("dicts", DICTS);
    assert!(run.stdout.contains("{1, 2, 3, 100, 10} set() {'banana', 'cherry', 'apple'}"), "{}", run.stdout);
    assert!(run.stdout.contains("{1, 2, 3, 100, 36, 8, 9, 10, 20, 52}"), "{}", run.stdout);
}

#[test]
fn dicts_and_sets_are_freed() {
    let run = run_c_with("dicts-free", DICTS, true);
    assert!(run.stderr.contains("lotml: 0 cells live at exit"), "{}", run.stderr);
}

#[test]
fn a_missing_key_stops_the_program() {
    for (name, body) in [("get", "print(d[\"z\"])"), ("remove", "var s = {1}\n    s.remove(2)")] {
        let source = format!("fn main():\n    d = {{\"a\": 1}}\n    print(\"before\")\n    {body}\n");
        let run = parity(&format!("dict-panic-{name}"), &source);
        assert_eq!(run.code, Some(101), "{name}");
    }
}
