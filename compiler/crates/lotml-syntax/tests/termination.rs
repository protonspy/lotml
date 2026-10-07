//! The parser always finishes: every loop reads a token or stops (plans/frontend-robustness.md
//! 1.2). Checked on token soup, where a loop that could go round without reading would hang.

use std::sync::mpsc;
use std::time::Duration;

use lotml_syntax::parse;

const WORDS: &[&str] = &[
    "fn",
    "type",
    "impl",
    "trait",
    "test",
    "from",
    "import",
    "if",
    "elif",
    "else",
    "for",
    "in",
    "while",
    "match",
    "case",
    "return",
    "var",
    "lambda",
    "not",
    "and",
    "or",
    "fail",
    "pass",
    "x",
    "Y",
    "1",
    "2.5",
    "\"s\"",
    "f\"{x}\"",
    "'c",
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    ":",
    ",",
    "=",
    "==",
    "+",
    "-",
    "*",
    "&",
    "|",
    "?",
    "??",
    "->",
    "!",
    ".",
    "@",
    "\\",
    "#c",
    "é",
    "\n",
    "\n    ",
    "\n        ",
    "\n  ",
    "\t",
];

/// `count` pieces of soup, each up to 80 words, from a fixed seed.
fn soup(count: usize) -> Vec<String> {
    let mut state: u64 = 0x2545_f491_4f6c_dd1d;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    (0..count)
        .map(|_| {
            let len = (next() % 80) as usize;
            (0..len).map(|_| WORDS[(next() % WORDS.len() as u64) as usize]).collect::<Vec<_>>().join(" ")
        })
        .collect()
}

#[test]
fn the_parser_finishes_on_token_soup() {
    let (done, finished) = mpsc::channel();
    std::thread::Builder::new()
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            for text in soup(20_000) {
                let parsed = parse(&text);
                assert!(
                    parsed
                        .errors
                        .iter()
                        .all(|e| (e.span.end as usize) <= text.len() && text.is_char_boundary(e.span.end as usize)),
                    "{text:?}"
                );
            }
            done.send(()).expect("the test is waiting");
        })
        .expect("a worker thread");
    finished.recv_timeout(Duration::from_secs(120)).expect("every parse finished in time");
}
