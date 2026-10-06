//! Methods and the `inout`, `sink` and `var` conventions on the C target: a callee changes its
//! caller's value only through `inout`, and a value shared with another binding is copied
//! before it changes (R1.2, R3.5).

mod common;

use common::{parity, run_c_with};

const METHODS: &str = include_str!("programs/methods.lotml");

#[test]
fn methods_and_conventions_behave_as_in_python() {
    let run = parity("methods", METHODS);
    assert!(run.stdout.starts_with("7 6 Counter(count=7, log=['+1', '+5', '+1'])"), "{}", run.stdout);
}

#[test]
fn methods_free_what_they_take() {
    let run = run_c_with("methods-free", METHODS, true);
    assert!(run.stderr.contains("lotml: 0 cells live at exit"), "{}", run.stderr);
}
