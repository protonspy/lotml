//! Read `{"id", "code"}` lines and report each program the formatter breaks: one that no longer
//! parses, parses to another tree, loses a comment, or changes when formatted again.

use std::io::{BufRead, Write};

use lotml_syntax::parse;

/// The tree's debug form with every span erased, so two layouts of one program compare equal.
pub fn shape(source: &str) -> String {
    let debug = format!("{:?}", parse(source).module);
    let mut out = String::with_capacity(debug.len());
    let mut rest = debug.as_str();
    while let Some(at) = rest.find("Span { start: ") {
        out.push_str(&rest[..at]);
        let after = &rest[at..];
        let end = after.find('}').map_or(after.len(), |i| i + 1);
        out.push('_');
        rest = &after[end..];
    }
    out.push_str(rest);
    out
}

fn main() {
    let mut out = std::io::stdout().lock();
    let (mut programs, mut broken) = (0, 0);
    for line in std::io::stdin().lock().lines() {
        let line = line.expect("a line");
        let value: serde_json::Value = serde_json::from_str(&line).expect("a JSON line");
        let code = value["code"].as_str().unwrap_or("");
        let Ok(formatted) = lotml_fmt::format(code) else { continue };
        programs += 1;
        let reparsed = parse(&formatted);
        let problem = if !reparsed.errors.is_empty() {
            Some(format!("does not parse: {}", reparsed.errors[0].message))
        } else if shape(code) != shape(&formatted) {
            Some("another tree".to_string())
        } else if reparsed.comments.len() != parse(code).comments.len() {
            Some("lost a comment".to_string())
        } else if lotml_fmt::format(&formatted).ok().as_deref() != Some(formatted.as_str()) {
            Some("not idempotent".to_string())
        } else {
            None
        };
        if let Some(problem) = problem {
            broken += 1;
            let row = serde_json::json!({"id": value["id"], "problem": problem, "formatted": formatted});
            writeln!(out, "{row}").expect("stdout");
        }
    }
    eprintln!("{programs} programs formatted, {broken} broken");
}
