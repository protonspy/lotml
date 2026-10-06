//! The rest of the prelude on the C target: `round`, `bool`, `hash`, `pow`, `divmod`, the wrapping
//! operations, `isqrt`, `gcd`, `Heap`, the `math` module and the sized integers, each as Python
//! computes it, and stopping where Python raises (R1.2, R2.1).

mod common;

use common::{parity, run_c_with};

const PRELUDE: &str = include_str!("programs/prelude.lotml");

#[test]
fn the_prelude_computes_as_python_does() {
    let run = parity("prelude", PRELUDE);
    assert!(run.stdout.starts_with("2 4 -2 0 7 -8\n2.67 1200.0 -0.0"), "{}", run.stdout);
}

#[test]
fn the_prelude_frees_what_it_takes() {
    let run = run_c_with("prelude-free", PRELUDE, true);
    assert!(run.stderr.contains("lotml: 0 cells live at exit"), "{}", run.stderr);
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
    ] {
        let source = format!(
            "import math\n\nfn f(n: int) -> int:\n    return n\n\nfn main():\n    n = f(1)\n    print(\"before\")\n    print({expr})\n"
        );
        let run = parity(&format!("prelude-trap-{name}"), &source);
        assert_eq!(run.code, Some(101), "{name}: {expr}");
        assert_eq!(run.stdout, "before\n", "{name}");
    }
}
