//! Sizes a program can ask for that no memory or buffer holds: the C target stops as Python does,
//! or with an error of its own past its limits, and never reads or writes outside what it
//! allocated.

mod common;

use common::{parity, run_c};

#[test]
fn a_list_too_long_for_memory_stops_with_memory_error() {
    for (name, body) in [
        ("repeat", "xs = [7] * 2305843009213693953\n    print(len(xs))"),
        ("repeat-store", "var xs = [7] * 2305843009213693953\n    xs[300000] = 1\n    print(xs[300000])"),
    ] {
        let source = format!("fn main():\n    print(\"before\")\n    {body}\n");
        let run = parity(&format!("limit-{name}"), &source);
        assert_eq!(run.code, Some(101), "{name}");
        assert_eq!(run.stdout, "before\n", "{name}");
        assert!(run.stderr.contains("MemoryError"), "{name}: {}", run.stderr);
    }
}

#[test]
fn a_long_float_precision_is_written_in_full() {
    let source = "fn main():\n    x = 1.5\n    s = f\"{x:.600f}\"\n    print(len(s), s[-3:])\n    \
                  g = f\"{x * 3.0:.5000g}\"\n    print(len(g), f\"{2.0 / 3.0:.400e}\"[-8:], f\"{1e300:,.30f}\"[:12])\n";
    let run = parity("limit-precision", source);
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert!(run.stdout.starts_with("602 000\n"), "{}", run.stdout);
}

#[test]
fn a_width_or_precision_past_the_limit_stops_with_value_error() {
    let source = "fn main():\n    x = 1.5\n    print(\"before\")\n    print(f\"{x:.20000f}\")\n";
    let run = run_c("limit-spec", source);
    assert_eq!(run.code, Some(101));
    assert!(run.stderr.contains("Too many decimal digits in format string"), "{}", run.stderr);
}
