//! Pathological nesting is a diagnostic, not a stack crash. The parser bounds its recursion; the
//! binary runs it on a large stack (see `lotml`'s `main`), which these tests mirror so they check
//! the depth guard, not a particular stack size.

use lotml_syntax::parse;

/// Parse on a thread with the stack the compiler gives its work.
fn parsed_errors(source: String) -> Vec<String> {
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(move || parse(&source).errors.into_iter().map(|e| e.message).collect::<Vec<_>>())
        .expect("a worker thread")
        .join()
        .expect("parsing did not crash")
}

fn refuses_deep_nesting(source: String) {
    assert!(parsed_errors(source).iter().any(|m| m.contains("nests too deeply")));
}

#[test]
fn deeply_nested_parentheses_do_not_crash() {
    refuses_deep_nesting(format!("fn f() -> int:\n    return {}1{}\n", "(".repeat(2_000), ")".repeat(2_000)));
}

#[test]
fn deeply_nested_blocks_do_not_crash() {
    let mut source = String::from("fn f() -> int:\n");
    for i in 0..2_000 {
        source.push_str(&"    ".repeat(i + 1));
        source.push_str("if True:\n");
    }
    refuses_deep_nesting(source);
}

#[test]
fn deeply_nested_types_do_not_crash() {
    refuses_deep_nesting(format!("fn f(x: {}int{}) -> int:\n    return 1\n", "[".repeat(2_000), "]".repeat(2_000)));
}
