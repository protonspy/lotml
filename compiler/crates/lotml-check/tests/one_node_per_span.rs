//! The checker keys the type of each expression by its span, which is sound only while no two
//! expressions share one: a desugared node reusing its source's span would overwrite the type
//! recorded for it (plans/frontend-robustness.md 2.1). A debug build asserts it as each type is
//! recorded; these tests run the checker where a shared span could come from, so the assertion
//! sees them.

use lotml_check::check_resolved;
use lotml_syntax::parse;

/// The programs of the corpus (`harness/results/corpus/corpus.jsonl`).
fn corpus() -> Vec<(String, String)> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../harness/results/corpus/corpus.jsonl");
    let text = std::fs::read_to_string(path).expect("the corpus");
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let row: serde_json::Value = serde_json::from_str(line).expect("a JSON row");
            (row["task"].as_str().unwrap_or("").to_string(), row["lotml"].as_str().unwrap_or("").to_string())
        })
        .collect()
}

fn checked(text: &str) -> lotml_check::Checked {
    check_resolved(&parse(text).module, text)
}

#[test]
#[cfg_attr(not(debug_assertions), ignore = "the assertion it relies on is a debug build's")]
fn every_corpus_program_types_each_span_from_one_expression() {
    let programs = corpus();
    assert!(programs.len() > 500, "the corpus holds its programs");
    for (task, text) in &programs {
        let result = std::panic::catch_unwind(|| checked(text));
        assert!(result.is_ok(), "{task}: two expressions share a span");
    }
}

#[test]
fn the_places_the_parser_builds_one_node_from_another_keep_their_own_spans() {
    // A parenthesized expression is one node with the parentheses' span, a lone generator
    // argument is its own node, a negative literal types its digits once, an f-string's fields
    // and an `if` expression's parts each have theirs.
    let program = "fn f(xs: [int]) -> int:\n    a = (xs[0])\n    b = sum(x for x in xs)\n    c = -128\n    \
                   d: i8 = -128\n    e = f\"{a}{(b)}{c!r:>4}\"\n    g = a if a < b < c else -a\n    h = not (a == b)\n    \
                   i = xs[0:1][0]\n    var j = 0\n    j += (a)\n    print(d, e, h, i)\n    return a + b + c + g + j\n";
    let result = checked(program);
    assert!(result.diagnostics.iter().all(|d| d.severity != lotml_diag::Severity::Error), "{:?}", result.diagnostics);
}

#[test]
fn text_the_parser_could_not_read_is_typed_as_an_error_wherever_it_is_missing() {
    // Cut after a trailing comma, a dict's missing key and missing value are two error nodes at
    // the same empty span: both typed as errors, so the one the map keeps is the other's type.
    let text = "fn f() -> int:\n    d = {\"a\": 1,";
    let result = checked(text);
    let end = lotml_syntax::span::Span::new(text.len(), text.len());
    assert_eq!(result.types.get(&end), Some(&lotml_check::ty::Ty::Error));
}
