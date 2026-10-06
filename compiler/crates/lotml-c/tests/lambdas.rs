//! Lambdas and function values on the C target: copies captured when the lambda is made, keys
//! computed once per element, and the prelude that takes a function (R1.2).

mod common;

use common::{parity, run_c_with};

const LAMBDAS: &str = include_str!("programs/lambdas.lotml");

#[test]
fn lambdas_and_function_values_behave_as_in_python() {
    let run = parity("lambdas", LAMBDAS);
    assert!(run.stdout.contains("\n10 2\n"), "a lambda captures a copy: {}", run.stdout);
}

#[test]
fn closures_and_what_they_capture_are_freed() {
    let run = run_c_with("lambdas-free", LAMBDAS, true);
    assert!(run.stderr.contains("lotml: 0 cells live at exit"), "{}", run.stderr);
}
