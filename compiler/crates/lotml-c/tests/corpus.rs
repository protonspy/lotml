//! What the corpus met on the C target: a name declared in each branch, an optional compared with
//! a value, `in` over a tuple, a list unpacked into names, a tuple walked and zipped, prelude
//! functions passed as values, keyword arguments of a `str` method (R1.2).

mod common;

use common::{parity, run_c_with};

const CORPUS: &str = include_str!("programs/corpus.lotml");

#[test]
fn what_the_corpus_met_behaves_as_in_python() {
    let run = parity("corpus", CORPUS);
    assert!(run.stdout.starts_with("[1, 2, 3, 4, 5, 6]\n"), "{}", run.stdout);
}

#[test]
fn what_the_corpus_met_frees_what_it_takes() {
    let run = run_c_with("corpus-free", CORPUS, true);
    assert!(run.stderr.contains("lotml: 0 cells live at exit"), "{}", run.stderr);
}

#[test]
fn unpacking_a_list_of_the_wrong_length_stops_as_python_does() {
    let source = "fn f(n: int) -> [int]:\n    return [n, n]\n\nfn main():\n    print(\"before\")\n    a, b, c = f(1)\n    print(a)\n";
    let run = parity("unpack-length", source);
    assert_eq!(run.code, Some(101));
    assert!(run.stderr.contains("not enough values to unpack (expected 3, got 2)"), "{}", run.stderr);
}
