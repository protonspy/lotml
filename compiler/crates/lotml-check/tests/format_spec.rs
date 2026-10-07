//! An f-string field's format spec is checked against the value it formats, as Python would refuse
//! it when the program runs (plans/frontend-robustness.md 2.2): E0223.

use lotml_check::check_source;

fn codes(source: &str) -> Vec<String> {
    check_source(source)
        .into_iter()
        .filter(|d| d.severity == lotml_diag::Severity::Error)
        .map(|d| d.code.to_string())
        .collect()
}

fn field(declarations: &str, field: &str) -> Vec<String> {
    codes(&format!("fn main():\n{declarations}    print(f\"{{{field}}}\")\n"))
}

const VALUES: &str = "    n = 42\n    x = 2.5\n    s = \"ab\"\n    b = True\n    xs = [1, 2]\n";

#[test]
fn a_spec_the_value_takes_is_accepted() {
    for f in
        ["n:>5", "n:,d", "n:#x", "n:.2f", "x:.3e", "x:+08.2f", "s:*^10", "s:05", "b:>5", "n!r:>6", "xs", "xs!r:>12"]
    {
        assert_eq!(field(VALUES, f), Vec::<String>::new(), "{{{f}}}");
    }
}

#[test]
fn a_spec_the_value_refuses_is_e0223() {
    for f in ["s:d", "n:.2", "x:x", "s:+", "n:,x", "xs:>5", "n:5.", "n!r:d", "n:>{n}", "x:99999"] {
        assert_eq!(field(VALUES, f), ["E0223"], "{{{f}}}");
    }
}

#[test]
fn the_message_says_what_python_would_raise() {
    let found = check_source("fn main():\n    s = \"ab\"\n    print(f\"{s:d}\")\n");
    let error = found.iter().find(|d| d.code == "E0223").expect("E0223");
    assert!(error.message.contains("Unknown format code 'd' for object of type 'str'"), "{}", error.message);
}

#[test]
fn a_value_whose_type_is_open_is_left_to_the_run() {
    let source = "fn show[T](value: T) -> str:\n    return f\"{value:>5}\"\n\nfn maybe(x: int?) -> str:\n    return f\"{x:>5}\"\n";
    assert_eq!(codes(source), Vec::<String>::new());
}

#[test]
fn no_corpus_program_is_refused_for_its_specs() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../harness/results/corpus/corpus.jsonl");
    let text = std::fs::read_to_string(path).expect("the corpus");
    for line in text.lines().filter(|l| !l.trim().is_empty()) {
        let row: serde_json::Value = serde_json::from_str(line).expect("a JSON row");
        let program = row["lotml"].as_str().unwrap_or("");
        let refused: Vec<String> =
            check_source(program).into_iter().filter(|d| d.code == "E0223").map(|d| d.message).collect();
        assert!(refused.is_empty(), "{}: {refused:?}", row["task"]);
    }
}
