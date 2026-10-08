//! A check timed after a one-line body edit (plans/build-and-check-speed.md 2.1): one large
//! generated file that imports from generated Python interfaces, checked through `lotml-db` from
//! an empty database and again after one line of one body changed; and, for scale, the file
//! parsed, its interfaces read and the file checked, each on its own. Each `runs` times. A JSON
//! line describing the file, then one per step with the seconds of each run.
//!
//!     cargo run --release -p lotml-db --example check_time -- [functions] [runs]

use std::time::Instant;

use lotml_db::{Database, SourceFile, diagnostics};
use salsa::Setter;

/// The Python modules the file imports from, and how many functions each interface declares.
const MODULES: usize = 4;
const SIGNATURES: usize = 200;

fn main() {
    let mut args = std::env::args().skip(1);
    let functions: usize = args.next().map_or(250, |f| f.parse().expect("a number of functions"));
    let runs: usize = args.next().map_or(5, |r| r.parse().expect("a number of runs"));
    let interfaces = interfaces();
    let edited = functions / 2;
    let text = program(functions, edited, 1);
    let other = program(functions, edited, 2);
    assert_eq!(text.lines().zip(other.lines()).filter(|(a, b)| a != b).count(), 1, "an edit of one line");
    println!(
        "{{\"lines\": {}, \"functions\": {functions}, \"modules\": {MODULES}, \"signatures\": {}}}",
        text.lines().count(),
        MODULES * SIGNATURES
    );
    let step = |name: &str, seconds: Vec<f64>| println!("{{\"step\": {name:?}, \"seconds\": {seconds:?}}}");
    step("parse", (0..runs).map(|_| timed(|| lotml_syntax::parse(&text))).collect());
    let read = || -> lotml_check::Interfaces {
        interfaces.iter().map(|(module, text)| (module.clone(), lotml_check::interface_of(module, text).0)).collect()
    };
    step("interfaces", (0..runs).map(|_| timed(read)).collect());
    let module = lotml_syntax::parse(&text).module;
    let read = read();
    step("check", (0..runs).map(|_| timed(|| lotml_check::check_resolved_with(&module, &text, &read))).collect());
    let cold = (0..runs)
        .map(|_| {
            let db = Database::default();
            let file = SourceFile::create(&db, "large.lot".into(), text.clone(), interfaces.clone());
            timed(|| clean(diagnostics(&db, file)))
        })
        .collect();
    step("cold", cold);
    let mut db = Database::default();
    let file = SourceFile::create(&db, "large.lot".into(), text.clone(), interfaces.clone());
    clean(diagnostics(&db, file));
    let edit = (0..runs)
        .map(|k| {
            let next = if k % 2 == 0 { &other } else { &text };
            file.set_text(&mut db).to(next.clone());
            timed(|| clean(diagnostics(&db, file)))
        })
        .collect();
    step("edit", edit);
}

/// The seconds `work` took.
fn timed<T>(work: impl FnOnce() -> T) -> f64 {
    let started = Instant::now();
    std::hint::black_box(work());
    started.elapsed().as_secs_f64()
}

/// A file that checks without an error, so what is timed is a check that went all the way.
fn clean(found: &[lotml_diag::Diagnostic]) -> usize {
    let errors: Vec<_> = found.iter().filter(|d| d.severity == lotml_diag::Severity::Error).collect();
    assert!(errors.is_empty(), "the generated file has errors: {:?}", errors.first().map(|d| (&d.code, &d.message)));
    found.len()
}

/// The name function `i` imports, spread over every module.
fn imported(i: usize) -> (usize, String) {
    let module = i % MODULES;
    (module, format!("g{module}_{}", (i / MODULES) % SIGNATURES))
}

/// `functions` functions, each a loop, a call into Python and a call to the one before it; the
/// function `edited` adds `step` in its loop, so two values of `step` differ by one line.
fn program(functions: usize, edited: usize, step: usize) -> String {
    let mut names: Vec<Vec<String>> = vec![Vec::new(); MODULES];
    for i in 0..functions {
        let (module, name) = imported(i);
        if !names[module].contains(&name) {
            names[module].push(name);
        }
    }
    let mut out = String::new();
    for (module, names) in names.iter().enumerate().filter(|(_, n)| !n.is_empty()) {
        out += &format!("from py.m{module} import {}\n", names.join(", "));
    }
    for i in 0..functions {
        let add = if i == edited { step } else { 1 };
        let (_, name) = imported(i);
        out += &format!("\nfn f{i}(n: int, s: str) -> int ! PyError:\n");
        out += "    var total = n\n";
        out += "    for k in range(10):\n";
        out += &format!("        total += k * {add}\n");
        out += &format!("    t = {name}(s, n)?\n");
        out += "    if len(t) > n:\n";
        out += "        total -= 1\n";
        out +=
            &if i == 0 { "    return total\n".to_string() } else { format!("    return total + f{}(n, s)?\n", i - 1) };
    }
    out
}

/// Each module's name with the text of its interface.
fn interfaces() -> Vec<(String, String)> {
    (0..MODULES)
        .map(|module| {
            let text: String = (0..SIGNATURES)
                .map(|k| format!("fn g{module}_{k}(text: str, count: int) -> str ! PyError\n"))
                .collect();
            (format!("py.m{module}"), text)
        })
        .collect()
}
