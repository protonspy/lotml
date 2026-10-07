//! Numbers and `bool` on the LLVM target: the Python target's results, every integer operation
//! checked against its type's range, and a broken invariant stopping the program where it broke
//! (specs/llvm-backend R2.1–R2.4).

mod common;

use common::{panics, parity};

#[test]
fn int_arithmetic_floors_and_divides_as_python_does() {
    parity(
        "ints",
        "fn main():\n    a = 17\n    b = -5\n    print(a + b, a - b, a * b, a / b, a // b, a % b, -a // 4, -a % 4, a ** 2)\n    \
         print(a << 3, a >> 2, a & b, a | b, a ^ b, -a, abs(b), ~a, min(a, b), max(a, b))\n",
    );
}

#[test]
fn float_arithmetic_is_ieee_and_floors_as_python_does() {
    parity(
        "floats",
        "fn main():\n    x = 7.5\n    y = -2.0\n    print(x + y, x - y, x * y, x / y, x // y, x % y, x ** y, -x, abs(y))\n    \
         print(x < y, x == 7.5, 0.1 + 0.2, 1e16, 1.0, -0.0, 1e308 * 10.0, float(3), int(x), int(-x))\n",
    );
}

#[test]
fn sized_integers_compute_and_convert_within_their_range() {
    parity(
        "sized",
        "fn main():\n    a: i8 = 100\n    b: i8 = 27\n    c: u8 = 200\n    d: u8 = 55\n    e: i32 = 2000000000\n    \
         f: u64 = 18446744073709551615\n    print(a + b, c + d, e // 7, e % 7, f // 2, f % 10, f > u64(1))\n    \
         print(i32(e), u8(200), i64(c), u64(e), i8(-128), float(c), f32(1.5) * f32(2.0))\n",
    );
}

#[test]
fn comparisons_and_bools_answer_as_python_does() {
    parity(
        "compare",
        "fn main():\n    a = 3\n    b = 4\n    big: u64 = 18446744073709551615\n    t = a < b\n    \
         print(a < b, a <= b, a > b, a >= b, a == b, a != b, big > u64(1), t == True, not t, t and a == 3, t or b == 0)\n    \
         print(-0.0 == 0.0, 2.5 >= 2.5, 1.0 != 1.5)\n",
    );
}

#[test]
fn an_int_that_overflows_stops_the_program_naming_where() {
    panics(
        "overflow",
        "fn bump(x: int) -> int:\n    return x + 1\n\nfn main():\n    print(\"before\")\n    print(bump(9223372036854775807))\n",
        "before\n",
        "Overflow: the result does not fit in int",
        2,
        "bump",
    );
}

#[test]
fn a_sized_integer_that_leaves_its_range_stops_the_program() {
    panics(
        "i8",
        "fn main():\n    a: i8 = 127\n    b: i8 = 1\n    print(a + b)\n",
        "",
        "Overflow: the result does not fit in i8",
        4,
        "main",
    );
    panics(
        "u8",
        "fn main():\n    a: u8 = 0\n    b: u8 = 1\n    print(a - b)\n",
        "",
        "Overflow: the result does not fit in u8",
        4,
        "main",
    );
    panics(
        "u64",
        "fn main():\n    a: u64 = 18446744073709551615\n    print(a * u64(2))\n",
        "",
        "Overflow: the result does not fit in u64",
        3,
        "main",
    );
    panics(
        "convert",
        "fn main():\n    n = 300\n    print(u8(n))\n",
        "",
        "Overflow: the result does not fit in u8",
        3,
        "main",
    );
    panics(
        "neg",
        "fn main():\n    n = -9223372036854775807 - 1\n    print(-n)\n",
        "",
        "Overflow: the result does not fit in int",
        3,
        "main",
    );
}

#[test]
fn a_division_by_zero_stops_the_program_as_python_names_it() {
    let zero = |name: &str, expression: &str, message: &str| {
        let source = format!("fn main():\n    a = 7\n    b = 0\n    x = 1.5\n    y = 0.0\n    print({expression})\n");
        panics(name, &source, "", &format!("ZeroDivisionError: {message}"), 6, "main");
    };
    zero("floordiv", "a // b", "integer division or modulo by zero");
    zero("mod", "a % b", "integer division or modulo by zero");
    zero("truediv", "a / b", "division by zero");
    zero("float", "x / y", "float division by zero");
}

#[test]
fn todo_and_a_failed_assert_stop_the_program() {
    panics(
        "todo",
        "fn later() -> int:\n    todo()\n\nfn main():\n    print(later())\n",
        "",
        "Todo: not written yet",
        2,
        "later",
    );
    panics(
        "assert",
        "fn main():\n    x = 2\n    print(x)\n    assert x > 3\n",
        "2\n",
        "TestFailure: x > 3\n  left:  2\n  right: 3",
        4,
        "main",
    );
}
