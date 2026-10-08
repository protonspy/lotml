//! Dicts and sets on the LLVM target: insertion order for dicts, CPython's hash and table order for
//! sets, under `PYTHONHASHSEED=0`; a missing key, an index and the other broken invariants stopping
//! the program (specs/llvm-parity R1.3, R2.1). A set display of three or more constants iterates
//! as CPython 3.12 and 3.13 build it, the versions CI and the harness run.

mod common;

use common::{frees_everything, panics, parity};

const DICTS: &str = include_str!("programs/dicts.lotml");

#[test]
fn dicts_and_sets_print_and_iterate_in_python_s_order() {
    let Some([run, _]) = parity("dicts", DICTS) else { return };
    assert!(run.stdout.contains("{1, 2, 3, 100, 10} set() {'banana', 'apple', 'cherry'}"), "{}", run.stdout);
    assert!(run.stdout.contains("{1, 2, 3, 100, 36, 8, 9, 10, 20, 52}"), "{}", run.stdout);
}

#[test]
fn dicts_and_sets_are_freed() {
    frees_everything("dicts-free", DICTS);
}

#[test]
fn the_length_of_a_set_counts_what_it_holds_after_a_removal() {
    let Some([run, _]) = parity(
        "set-length",
        "fn main():\n    var s = {1, 2, 3, 4}\n    s.remove(2)\n    s.discard(9)\n    s.add(5)\n    \
         var d = {\"a\": 1, \"b\": 2}\n    d.pop(\"a\")\n    print(len(s), len(d), s, d)\n",
    ) else {
        return;
    };
    assert_eq!(run.stdout, "4 1 {1, 3, 4, 5} {'b': 2}\n");
}

#[test]
fn a_missing_key_stops_the_program() {
    for (name, body) in [("get", "print(d[\"z\"])"), ("remove", "var s = {1}\n    s.remove(2)")] {
        let source = format!("fn main():\n    d = {{\"a\": 1}}\n    print(\"before\")\n    {body}\n");
        if let Some(runs) = parity(&format!("dict-panic-{name}"), &source) {
            assert!(runs.iter().all(|r| r.code == Some(101)), "{name}");
        }
    }
}

#[test]
fn a_broken_invariant_names_its_kind_line_and_function() {
    panics(
        "index-place",
        "fn pick(xs: [int], i: int) -> int:\n    return xs[i]\n\nfn main():\n    print(\"before\")\n    print(pick([1], 4))\n",
        "before\n",
        "IndexError: list index out of range",
        2,
        "pick",
    );
    panics(
        "key-place",
        "fn look(d: {str: int}) -> int:\n    return d[\"k\"]\n\nfn main():\n    print(\"before\")\n    print(look({}))\n",
        "before\n",
        "KeyError",
        2,
        "look",
    );
}
