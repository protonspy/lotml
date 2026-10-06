//! The `test` blocks on the C target: each run under a catcher, and reported as the Python
//! target's `run_tests` reports them, every value shown as its `show` writes it (R1.4).

mod common;

use common::run_c_tests;
use serde_json::{Value, json};

const MODULE: &str = r#"type Item(name: str, price: f64)
type Shape = Circle(f64) | Square(side: f64) | Dot
type Problem = Missing(name: str) | Empty

fn first(items: [Item]) -> Item ! Problem:
    if len(items) == 0:
        fail Missing("none")
    return items[0]

fn check(n: int):
    assert n > 1, (n, "small")

test "passes":
    print("printed by a test")
    assert first([Item("a", 1.5)])?.price == 1.5

test "items":
    assert [Item("a\n\"q\"", 1.0)] == [Item("é", 2.0)]

test "shapes":
    assert (Circle(1.0), Square(2.0), Dot, (1,), Ok(2)) == (Dot, Dot, Dot, (2,), Ok(3))

test "collections":
    assert ({30, 4, 200}, {"k": [None, 1]}, Heap([3, 1, 2])) == ({1}, {"k": [None]}, Heap([1]))

test "helper":
    check(0)

test "plain":
    assert len("ab") > 5

test "error":
    x = first([])?
    print(x)

test "panic":
    xs = [1, 2]
    print(xs[5])

test "after":
    assert True
"#;

fn report() -> Vec<Value> {
    let run = run_c_tests("report", MODULE);
    assert_eq!(run.code, Some(0), "{}", run.stderr);
    assert!(run.stdout.starts_with("printed by a test\n"), "{}", run.stdout);
    let last = run.stdout.lines().last().unwrap_or("");
    serde_json::from_str::<Vec<Value>>(last).unwrap_or_else(|e| panic!("{e}: {last}"))
}

#[test]
fn each_test_reports_its_outcome_with_the_values_shown() {
    let tests = report();
    let by_name = |name: &str| tests.iter().find(|t| t["name"] == name).unwrap_or_else(|| panic!("{name}")).clone();
    assert_eq!(by_name("passes"), json!({"name": "passes", "outcome": "pass"}));
    let items = by_name("items");
    assert_eq!(items["outcome"], "fail");
    assert_eq!(items["op"], "==");
    assert_eq!(items["left"], r#"[Item(name="a\n\"q\"", price=1.0)]"#);
    assert_eq!(items["right"], r#"[Item(name="é", price=2.0)]"#);
    assert_eq!(items["line"], 18);
    let shapes = by_name("shapes");
    assert_eq!(shapes["left"], "(Circle(1.0), Square(side=2.0), Dot, (1,), Ok(2))");
    let collections = by_name("collections");
    assert_eq!(collections["left"], r#"({200, 30, 4}, {"k": [None, 1]}, Heap([1, 2, 3]))"#);
    assert_eq!(collections["right"], r#"({1}, {"k": [None]}, Heap([1]))"#);
    let helper = by_name("helper");
    assert_eq!((helper["outcome"].as_str(), helper["line"].as_u64()), (Some("fail"), Some(11)));
    assert_eq!(helper["left"], "0");
    assert_eq!(helper["message"], r#"(0, "small")"#);
    let plain = by_name("plain");
    assert_eq!(plain["expression"], r#"len("ab") > 5"#);
    assert_eq!(by_name("error")["error"], r#"Missing(name="none")"#);
    let panic = by_name("panic");
    assert_eq!((panic["outcome"].as_str(), panic["kind"].as_str()), (Some("panic"), Some("IndexError")));
    assert_eq!(panic["trace"], json!([{"function": "__test_8", "line": 38}]));
    assert_eq!(by_name("after")["outcome"], "pass");
}
