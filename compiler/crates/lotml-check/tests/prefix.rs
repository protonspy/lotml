//! `check --prefix` (R29, R31): a partial file is completable, an error, or cannot tell yet —
//! and a prefix some continuation completes is never an error.

use lotml_check::{Verdict, check_prefix, check_source};

fn verdict(source: &str) -> Verdict {
    check_prefix(source).verdict
}

#[test]
fn a_correct_program_so_far_is_completable() {
    assert_eq!(verdict("fn f() -> int:\n    return 1\n"), Verdict::Completable);
    assert_eq!(verdict(""), Verdict::Completable);
}

#[test]
fn an_unfinished_line_cannot_be_told_yet() {
    assert_eq!(verdict("fn f() -> int:\n    return g("), Verdict::Unknown);
    assert_eq!(verdict("fn f(xs: [int]) -> int:\n    return xs.app"), Verdict::Unknown);
    assert_eq!(verdict("fn f() -> str:\n    return \"ab"), Verdict::Unknown);
}

#[test]
fn what_later_code_may_declare_is_held() {
    assert_eq!(verdict("fn f() -> int:\n    return g(1)\n"), Verdict::Unknown);
    assert_eq!(verdict("fn f(u: User) -> str:\n    return u.name\n"), Verdict::Unknown);
}

#[test]
fn a_body_still_being_written_may_return_later() {
    assert_eq!(verdict("fn f(x: int) -> int:\n    y = x + 1\n"), Verdict::Unknown);
    let earlier = check_prefix("fn f(x: int) -> int:\n    y = x + 1\n\nfn g() -> int:\n    return 1\n");
    assert_eq!(earlier.verdict, Verdict::Error);
    assert_eq!(earlier.errors[0].code, "E0209");
}

#[test]
fn an_error_no_continuation_removes_is_reported() {
    let found = check_prefix("fn f() -> int:\n    x = 1\n    x = 2\n    return x\n\nfn g");
    assert_eq!(found.verdict, Verdict::Error);
    assert_eq!(found.errors.iter().map(|d| d.code).collect::<Vec<_>>(), vec!["E0301"]);
}

#[test]
fn a_signature_being_typed_does_not_condemn_its_callers() {
    // `g` is called with two arguments while its parameter list is still being written.
    assert_ne!(verdict("fn f() -> int:\n    return g(1, 2)\n\nfn g(a"), Verdict::Error);
}

#[test]
fn no_prefix_of_a_corpus_program_is_an_error() {
    let corpus = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../research/tokens/corpus");
    let entries = std::fs::read_dir(&corpus).expect("the corpus, from the repository root");
    for entry in entries.flatten() {
        let Ok(text) = std::fs::read_to_string(entry.path().join("b.x")) else { continue };
        if check_source(&text).iter().any(|d| d.severity == lotml_diag::Severity::Error) {
            continue;
        }
        for (cut, _) in text.char_indices() {
            let found = check_prefix(&text[..cut]);
            assert_ne!(found.verdict, Verdict::Error, "{}[..{cut}]: {:#?}", entry.path().display(), found.errors);
        }
    }
}
