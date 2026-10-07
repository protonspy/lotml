//! The first increment's milestone (specs/llvm-backend R2.2): `add`, `fib`, `collatz` and
//! `mandelbrot` print on `--target llvm`, at `-O0` and at `-O2`, what they print on the Python
//! target; and `add`'s IR is checked in, so a change to the emitter shows in review.

mod common;

use std::path::Path;

use common::parity;

const ADD: &str = "fn add(a: f64, b: f64) -> f64:\n    return a + b\n\nfn main():\n    print(add(1.5, 2.25))\n";

const COLLATZ: &str = "fn steps(n: int) -> int:\n    var count = 0\n    var x = n\n    while x != 1:\n        if x % 2 == 0:\n            \
x = x // 2\n        else:\n            x = 3 * x + 1\n        count += 1\n    return count\n\n\
fn main():\n    var best = 0\n    var longest = 0\n    for n in range(1, 30000):\n        s = steps(n)\n        if s > longest:\n            \
longest = s\n            best = n\n    print(best, longest)\n";

#[test]
fn add_prints_what_the_python_target_prints() {
    if let Some([debug, _]) = parity("add", ADD) {
        assert_eq!(debug.stdout, "3.75\n");
    }
}

#[test]
fn fib_prints_what_the_python_target_prints() {
    parity("fib", include_str!("../../../../harness/benchmarks/fib.lotml"));
}

#[test]
fn collatz_prints_what_the_python_target_prints() {
    parity("collatz", COLLATZ);
}

#[test]
fn mandelbrot_prints_what_the_python_target_prints() {
    parity("mandelbrot", include_str!("../../../../harness/benchmarks/mandelbrot.lotml"));
}

#[test]
fn add_compiles_to_the_checked_in_ir() {
    let ll = lotml_llvm::compile(ADD, Path::new("add.lot")).expect("add compiles");
    let expected = include_str!("add.ll").replace("\r\n", "\n");
    assert_eq!(ll, expected, "the IR of add changed: review it, then update tests/add.ll");
}
