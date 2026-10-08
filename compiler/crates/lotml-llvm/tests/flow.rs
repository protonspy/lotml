//! Control flow and calls on the LLVM target (specs/llvm-backend R2.1, R2.2).

mod common;

use common::{panics, parity};

const FLOW: &str = "fn fib(n: int) -> int:\n    if n < 2:\n        return n\n    return fib(n - 1) + fib(n - 2)\n\n\
fn collatz(n: int) -> int:\n    var steps = 0\n    var x = n\n    while x != 1:\n        if x % 2 == 0:\n            x = x // 2\n        else:\n            x = 3 * x + 1\n        steps += 1\n    return steps\n\n\
fn main():\n    var total = 0\n    for i in range(10):\n        if i == 3:\n            continue\n        if i == 8:\n            break\n        total += i\n    print(total, fib(20), collatz(27))\n    \
for j in range(10, 0, -3):\n        print(j, end=\" \")\n    print()\n    for k in range(-2, 3, 2):\n        print(k, sep=\", \", end=\"|\")\n    print(\"done\", True, 2.5, sep=\"; \")\n";

#[test]
fn branches_loops_calls_and_recursion_run_as_on_python() {
    parity("flow", FLOW);
}

#[test]
fn a_range_near_the_end_of_int_stops_without_overflowing() {
    parity(
        "range-edge",
        "fn main():\n    for i in range(9223372036854775805, 9223372036854775807):\n        print(i)\n    \
         for j in range(9223372036854775800, 9223372036854775807, 5):\n        print(j)\n    \
         for k in range(-9223372036854775803, -9223372036854775807 - 1, -2):\n        print(k)\n",
    );
}

#[test]
fn a_range_with_a_zero_step_stops_the_program() {
    panics(
        "range-zero",
        "fn main():\n    s = 0\n    for i in range(0, 5, s):\n        print(i)\n",
        "",
        "ValueError: range() arg 3 must not be zero",
        3,
        "main",
    );
}

#[test]
fn an_early_return_inside_nested_loops_leaves_them_all() {
    parity(
        "nested",
        "fn find(target: int) -> int:\n    for i in range(10):\n        for j in range(10):\n            if i * j == target:\n                \
         return i * 100 + j\n    return -1\n\nfn main():\n    print(find(42), find(97))\n",
    );
}

#[test]
fn the_longest_elif_chain_lotml_reads_runs_as_on_python() {
    let mut source = String::from("fn pick(x: int) -> int:\n    if x == 0:\n        return 0\n");
    for k in 1..256 {
        source.push_str(&format!("    elif x == {k}:\n        return {k}\n"));
    }
    source.push_str("    else:\n        return -1\n\nfn main():\n    print(pick(0), pick(255), pick(256))\n");
    // On the stack the `lotml` binary gives the compiler, as tests/depth.rs of lotml-syntax does.
    std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(move || parity("elif-chain", &source))
        .expect("a worker thread")
        .join()
        .expect("the chain compiles and runs on both targets");
}
