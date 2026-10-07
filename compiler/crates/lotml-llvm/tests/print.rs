//! `print` on the LLVM target: numbers as CPython's `repr` writes them, through the runtime
//! (specs/llvm-backend R2.1, R2.5).

mod common;

use common::parity;

#[test]
fn floats_print_as_cpython_repr_writes_them() {
    parity(
        "repr",
        "fn main():\n    print(0.1 + 0.2, 1e16, 1e15, 123456789.125, 1.0 / 3.0, 5e-324, -0.0, 2.5e-5, 1e22, 100.0)\n    \
         a: f32 = 0.1\n    print(a, a * f32(3.0))\n",
    );
}

#[test]
fn print_takes_a_separator_an_end_and_string_literals() {
    parity(
        "print",
        "fn main():\n    print(\"hello\", 3, True, False, 2.5)\n    print(1, 2, 3, sep=\"-\")\n    print(\"no newline\", end=\"\")\n    \
         print()\n    print(\"tab\\there\", \"quote \\\"q\\\"\", \"utf-8: \u{e9}\u{4e2d}\")\n    print()\n    print(\"\", end=\"!\\n\")\n",
    );
}

#[test]
fn integers_of_every_width_print_their_value() {
    parity(
        "widths",
        "fn main():\n    a: i8 = -128\n    b: u8 = 255\n    c: i16 = -32768\n    d: u16 = 65535\n    e: i32 = -2147483648\n    \
         f: u32 = 4294967295\n    g: u64 = 18446744073709551615\n    h = -9223372036854775807 - 1\n    print(a, b, c, d, e, f, g, h)\n",
    );
}
