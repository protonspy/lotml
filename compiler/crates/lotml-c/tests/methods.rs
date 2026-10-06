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

#[test]
fn a_value_passed_both_by_value_and_inout_keeps_its_own_count() {
    let source = "type Bag(items: [int])\n\nimpl Bag:\n    fn absorb(inout self, other: Bag):\n        \
                  self.items.extend(other.items)\n\n\
                  fn grow(a: [int], inout b: [int]):\n    b.append(len(a))\n\n\
                  fn ignore(a: [int], inout b: [int]):\n    b.append(0)\n\n\
                  fn main():\n    var xs = [1, 2, 3]\n    grow(xs, &xs)\n    print(xs)\n    \
                  var ys = [1, 2]\n    ignore(ys, &ys)\n    var b = Bag([1])\n    b.absorb(b)\n    print(b)\n    \
                  var c = Bag([2])\n    c.absorb(c)\n";
    let run = parity("by-value-and-inout", source);
    assert_eq!(run.stdout, "[1, 2, 3, 3]\nBag(items=[1, 1])\n");
    let counted = run_c_with("by-value-and-inout-free", source, true);
    assert!(counted.stderr.contains("lotml: 0 cells live at exit"), "{}", counted.stderr);
}
