//! Check files and print their diagnostics; with `--jsonl`, read `{"id", "code"}` lines
//! from stdin and print one `{"id", "codes", "messages"}` line of its errors per program.

use std::io::{BufRead, Write};

use lotml_check::check_source;
use lotml_diag::Report;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().is_some_and(|a| a == "--jsonl") {
        let stdin = std::io::stdin();
        let mut out = std::io::stdout().lock();
        for line in stdin.lock().lines() {
            let line = line.expect("a line");
            let value: serde_json::Value = serde_json::from_str(&line).expect("a JSON line");
            let code = value["code"].as_str().unwrap_or("");
            let found = check_source(code);
            let found: Vec<_> = found.into_iter().filter(|d| d.severity == lotml_diag::Severity::Error).collect();
            let codes: Vec<&str> = found.iter().map(|d| d.code).collect();
            let messages: Vec<&str> = found.iter().map(|d| d.message.as_str()).collect();
            let row = serde_json::json!({"id": value["id"], "codes": codes, "messages": messages});
            writeln!(out, "{row}").expect("stdout");
        }
        return;
    }
    if args.first().is_some_and(|a| a == "--prefixes") {
        // Every prefix of a program without errors must never be rejected.
        let stdin = std::io::stdin();
        let mut out = std::io::stdout().lock();
        let (mut programs, mut prefixes) = (0, 0);
        for line in stdin.lock().lines() {
            let line = line.expect("a line");
            let value: serde_json::Value = serde_json::from_str(&line).expect("a JSON line");
            let code = value["code"].as_str().unwrap_or("");
            if check_source(code).iter().any(|d| d.severity == lotml_diag::Severity::Error) {
                continue;
            }
            programs += 1;
            for (cut, _) in code.char_indices().skip(1) {
                prefixes += 1;
                let result = lotml_check::check_prefix(&code[..cut]);
                if result.verdict == lotml_check::Verdict::Error {
                    let codes: Vec<&str> = result.errors.iter().map(|d| d.code).collect();
                    let messages: Vec<&str> = result.errors.iter().map(|d| d.message.as_str()).collect();
                    let row = serde_json::json!({"id": value["id"], "cut": cut, "codes": codes, "messages": messages, "tail": &code[cut.saturating_sub(40)..cut]});
                    writeln!(out, "{row}").expect("stdout");
                }
            }
        }
        eprintln!("{programs} programs, {prefixes} prefixes");
        return;
    }
    let mut failed = 0;
    for path in &args {
        let text = std::fs::read_to_string(path).expect("a readable file");
        let diagnostics = check_source(&text);
        if !diagnostics.is_empty() {
            failed += 1;
            let report = Report { file: path, text: &text, diagnostics };
            print!("{}", report.text(None));
        }
    }
    println!("{} of {} files have diagnostics", failed, args.len());
}
