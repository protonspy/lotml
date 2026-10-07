//! The rest of the prelude on the LLVM target: `round`, `bool`, `hash`, `pow`, `divmod`, the
//! wrapping operations, `isqrt`, `gcd`, `Heap`, the `math` module, the sized integers and the
//! conversions, each as Python computes it and stopping where Python raises; and a list a loop
//! stores into, still copied when shared (specs/llvm-parity R1.1, R1.2).

mod common;

use common::{frees_everything, parity};

const PRELUDE: &str = include_str!("programs/prelude.lotml");

#[test]
fn the_prelude_computes_as_python_does() {
    let Some([run, _]) = parity("prelude", PRELUDE) else { return };
    assert!(run.stdout.starts_with("2 4 -2 0 7 -8\n2.67 1200.0 -0.0"), "{}", run.stdout);
    frees_everything("prelude-free", PRELUDE);
}

#[test]
fn the_prelude_stops_where_python_raises() {
    for (name, expr) in [
        ("isqrt", "isqrt(-n)"),
        ("sqrt", "math.sqrt(-1.0 * float(n))"),
        ("log", "math.log(n - 1)"),
        ("pow-negative", "pow(2, -n)"),
        ("pow-modulus", "pow(3, n, n - 1)"),
        ("pow-inverse", "pow(2, -n, 4)"),
        ("divmod", "divmod(n, n - 1)"),
        ("divmod-float", "divmod(float(n), 0.0)"),
        ("factorial", "math.factorial(20 + n)"),
        ("factorial-negative", "math.factorial(-n)"),
        ("comb", "math.comb(67 + n, 34)"),
        ("comb-negative", "math.comb(-n, 2)"),
        ("round", "round(math.inf * float(n))"),
        ("floor", "math.floor(math.inf * float(n))"),
        ("exp", "math.exp(1000.0 * float(n))"),
        ("sized", "i8(127 + n)"),
        ("gcd", "gcd(-9223372036854775807 - n, 0)"),
        ("float-to-int", "int(math.inf * float(n))"),
    ] {
        let source = format!(
            "import math\n\nfn f(n: int) -> int:\n    return n\n\nfn main():\n    n = f(1)\n    print(\"before\")\n    print({expr})\n"
        );
        let Some(runs) = parity(&format!("prelude-trap-{name}"), &source) else { return };
        for run in &runs {
            assert_eq!(run.code, Some(101), "{name}: {expr}");
            assert_eq!(run.stdout, "before\n", "{name}");
        }
    }
}

#[test]
fn every_integer_operation_traps_outside_its_type() {
    for (name, expr) in [
        ("add", "9223372036854775807 + n"),
        ("sub", "-9223372036854775807 - n - n"),
        ("mul", "4611686018427387904 * (n + 1)"),
        ("pow", "2 ** (62 + n)"),
        ("shift", "n << 63"),
        ("neg", "-(-9223372036854775807 - n)"),
        ("floordiv", "(-9223372036854775807 - n) // -n"),
        ("abs", "abs(-9223372036854775807 - n)"),
    ] {
        let source = format!(
            "fn f(n: int) -> int:\n    return n\n\nfn main():\n    n = f(1)\n    print(\"before\")\n    print({expr})\n"
        );
        let Some(runs) = parity(&format!("trap-{name}"), &source) else { return };
        for run in &runs {
            assert_eq!(run.code, Some(101), "{name}: {expr}");
            assert_eq!(run.stdout, "before\n", "{name}");
        }
    }
}

#[test]
fn a_list_changed_in_a_loop_is_still_copied_when_shared() {
    let source = "fn fill(n: int) -> [[int]]:\n    base = [0] * n\n    var row = base\n    for i in range(n):\n        row[i] = i\n    \
                  var rows: [[int]] = []\n    var grid = [0] * n\n    for i in range(n):\n        grid[i] = i * 10\n        rows.append(grid)\n    \
                  var counts = [0] * 3\n    var j = 0\n    while j < 9:\n        counts[j % 3] += 1\n        if counts[0] > 1:\n            \
                  counts[1] = len(counts) + counts[0]\n        j += 1\n    return [base, row, counts] + rows\n\n\
                  fn main():\n    print(fill(3))\n";
    let Some([run, _]) = parity("hoist", source) else { return };
    assert_eq!(run.stdout, "[[0, 0, 0], [0, 1, 2], [3, 6, 3], [0, 0, 0], [0, 10, 0], [0, 10, 20]]\n");
    frees_everything("hoist-free", source);
}

#[test]
fn a_program_with_no_main_says_so() {
    let Some(clang) = common::clang() else { return };
    let run =
        common::run_llvm(&clang, "no-main", "fn helper() -> int:\n    return 1\n", lotml_llvm::driver::Level::Debug);
    assert_eq!(run.code, Some(2));
    assert!(run.stderr.contains("the program has no `fn main()`"), "{}", run.stderr);
}
