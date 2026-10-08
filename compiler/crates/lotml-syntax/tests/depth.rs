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

/// A long chain of operators builds a tree as deep as the chain, which every later pass recurses
/// over, so its length is bounded like nesting is.
fn refuses_a_long_chain(expression: String) {
    let errors = parsed_errors(format!("fn f(x: int) -> int:\n    return {expression}\n"));
    assert!(errors.iter().any(|m| m.contains("too long")), "{expression:.40}…: {errors:?}");
}

#[test]
fn long_operator_chains_do_not_crash() {
    refuses_a_long_chain(format!("{}x", "-".repeat(2_000)));
    refuses_a_long_chain(format!("{}x", "not ".repeat(2_000)));
    refuses_a_long_chain(format!("x{}", " ** x".repeat(2_000)));
    refuses_a_long_chain(format!("x{}", " + x".repeat(2_000)));
    refuses_a_long_chain(format!("x{}", " ?? x".repeat(2_000)));
}

#[test]
fn long_postfix_chains_do_not_crash() {
    refuses_a_long_chain(format!("x{}", ".y".repeat(2_000)));
    refuses_a_long_chain(format!("x{}", "()".repeat(2_000)));
    refuses_a_long_chain(format!("x{}", "[0]".repeat(2_000)));
}

#[test]
fn an_ordinary_chain_is_fine() {
    let errors = parsed_errors(format!("fn f(x: int) -> int:\n    return x{}\n", " + x".repeat(200)));
    assert!(errors.is_empty(), "{errors:?}");
}

/// An `if` with `branches` branches and an `else`.
fn chain(branches: usize) -> String {
    let mut source = String::from("fn pick(x: int) -> int:\n    if x == 0:\n        return 0\n");
    for k in 1..branches {
        source.push_str(&format!("    elif x == {k}:\n        return {k}\n"));
    }
    source.push_str("    else:\n        return -1\n");
    source
}

#[test]
fn a_long_elif_chain_is_refused_rather_than_nested_past_what_the_targets_run() {
    assert!(parsed_errors(chain(256)).is_empty(), "256 branches and an `else` nest as deep as lotml goes");
    let errors = parsed_errors(chain(257));
    assert!(errors.iter().any(|m| m.contains("more than 256 branches")), "{errors:?}");
    let errors = parsed_errors(chain(20_000));
    assert_eq!(errors.len(), 1, "reported once: {:?}", &errors[..errors.len().min(3)]);
}
