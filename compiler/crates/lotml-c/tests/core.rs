//! Functions, numbers, `bool`, control flow and `print` on the C target: what the Python target
//! prints, with integer arithmetic checked and panics at the lotml line (R1.2, R2.1–R2.3).

mod common;

use common::{parity, run_c};

#[test]
fn integer_arithmetic_gives_python_s_results() {
    let run = parity(
        "integers",
        "fn main():\n    print(7 + 3, 7 - 10, 6 * 7, 7 // 2, -7 // 2, 7 % 3, -7 % 3, 7 % -3, 2 ** 10, 1 << 4, \
         -16 >> 2, 6 & 3, 6 | 3, 6 ^ 3, ~5, -(-5))\n    print(abs(-4), min(3, 9), max(3, 9), 0 <= 5 < 10, 1 < 2 > 3)\n",
    );
    assert_eq!(run.stdout, "10 -3 42 3 -4 1 2 -2 1024 16 -4 2 7 5 -6 5\n4 3 9 True False\n");
}

#[test]
fn floats_print_as_python_s_repr() {
    let run = parity(
        "floats",
        "fn main():\n    print(7 / 2, 1 / 3, 0.1 + 0.2, 1e16, 1.5e-05, 2.0, -0.0, 1e22, 5e-324, 123456789.0)\n    \
         print(1e300 * 1e10, -(1e300 * 1e10), int(3.9), int(-3.9), float(7), 2.5 * 4.0, 10.0 // 3.0, -7.5 % 2.0, 2.0 ** 0.5)\n",
    );
    assert_eq!(
        run.stdout,
        "3.5 0.3333333333333333 0.30000000000000004 1e+16 1.5e-05 2.0 -0.0 1e+22 5e-324 123456789.0\n\
         inf -inf 3 -3 7.0 10.0 3.0 0.5 1.4142135623730951\n"
    );
}

#[test]
fn functions_and_control_flow_run_as_in_python() {
    let run = parity(
        "control",
        "fn fib(n: int) -> int:\n    if n < 2:\n        return n\n    return fib(n - 1) + fib(n - 2)\n\n\
         fn scaled(n: int, times: int = 2) -> int:\n    return n * times\n\n\
         fn main():\n    print(fib(20), scaled(5), scaled(5, times=3))\n    var total = 0\n    for i in range(10):\n        \
         if i % 2 == 0:\n            continue\n        if i > 7:\n            break\n        total += i\n    print(total)\n    \
         var n = 27\n    var steps = 0\n    while n != 1:\n        n = n // 2 if n % 2 == 0 else 3 * n + 1\n        steps += 1\n    \
         print(steps, True, False, None, \"text\")\n    for k in range(10, 0, -3):\n        print(k)\n    print()\n    \
         if steps > 200 and total == 16 or not True:\n        print(\"no\")\n    elif steps == 111:\n        print(\"yes\")\n    else:\n        print(\"else\")\n",
    );
    assert_eq!(run.stdout, "6765 10 15\n16\n111 True False None text\n10\n7\n4\n1\n\nyes\n");
}

#[test]
fn sized_integers_check_their_own_range() {
    let run = parity("sized", "fn main():\n    x: i8 = 100\n    print(x + 27)\n    y: u8 = 200\n    print(y + 55)\n");
    assert_eq!(run.stdout, "127\n255\n");
    parity("sized-overflow", "fn main():\n    x: i8 = 100\n    print(x + 28)\n");
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
        ("zero", "7 // (n - 1)"),
        ("mod-zero", "7 % (n - 1)"),
        ("div-zero", "7 / (n - 1)"),
        ("abs", "abs(-9223372036854775807 - n)"),
    ] {
        let source = format!("fn f(n: int) -> int:\n    return n\n\nfn main():\n    n = f(1)\n    print(\"before\")\n    print({expr})\n");
        let run = parity(&format!("trap-{name}"), &source);
        assert_eq!(run.code, Some(101), "{name}: {expr}");
        assert_eq!(run.stdout, "before\n", "{name}");
    }
}

#[test]
fn a_panic_names_the_lotml_line_and_function() {
    let run = parity("assert", "fn check(n: int):\n    assert n > 1\n\nfn main():\n    print(1)\n    check(0)\n");
    assert_eq!(run.code, Some(101));
    assert!(run.stderr.contains("line 2, in check"), "{}", run.stderr);
    assert!(run.c.contains("#line 2 "), "the C carries #line directives");
    let todo = parity("todo", "fn later() -> int:\n    return todo()\n\nfn main():\n    print(later())\n");
    assert!(todo.stderr.contains("line 2, in later"), "{}", todo.stderr);
}

#[test]
fn a_program_with_no_main_says_so() {
    let run = run_c("no-main", "fn helper() -> int:\n    return 1\n");
    assert_eq!(run.code, Some(2));
    assert!(run.stderr.contains("the program has no `fn main()`"), "{}", run.stderr);
}
