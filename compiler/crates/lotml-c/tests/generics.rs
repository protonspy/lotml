//! Generics, traits and `dyn` on the C target: a generic function or type is compiled once per
//! instantiation, a bounded type parameter calls the instance's own method, and a `dyn` value
//! calls through its type's table (R1.1, R1.2).

mod common;

use common::{parity, run_c_with};

const GENERICS: &str = include_str!("programs/generics.lotml");

#[test]
fn generics_traits_and_dyn_behave_as_in_python() {
    let run = parity("generics", GENERICS);
    assert!(run.stdout.starts_with("1 a 2.5\n"), "{}", run.stdout);
}

#[test]
fn generics_free_what_they_take() {
    let run = run_c_with("generics-free", GENERICS, true);
    assert!(run.stderr.contains("lotml: 0 cells live at exit"), "{}", run.stderr);
}
