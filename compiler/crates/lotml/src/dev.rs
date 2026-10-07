//! `lotml dev`: commands that build the harness guide's data, hidden from help.

use std::path::Path;

use lotml_diag::Severity;
use lotml_ide::diff::Declaration;
use lotml_ide::mutate::mutants;
use lotml_syntax::span::line_column;
use serde_json::json;

use crate::{Failure, exec, files, guide};

/// Print every mutant of `path`, in source order: as JSON, each with its operator, family,
/// declaration, span, line, replacement and the mutated text; or one line each.
pub fn mutate(path: &Path, as_json: bool) -> Result<bool, Failure> {
    let text = files::read(path)?;
    let found = mutants(&text);
    if as_json {
        let listed: Vec<_> = found
            .iter()
            .map(|m| {
                json!({
                    "operator": m.operator,
                    "family": m.family,
                    "declaration": m.declaration,
                    "start": m.span.start,
                    "end": m.span.end,
                    "line": line_column(&text, m.span.start).0,
                    "replacement": m.replacement,
                    "text": m.apply(&text),
                })
            })
            .collect();
        println!("{}", serde_json::Value::Array(listed));
    } else {
        for m in &found {
            let line = line_column(&text, m.span.start).0;
            println!("{line}: {} in {}: `{}` -> `{}`", m.operator, m.declaration, &text[m.span.range()], m.replacement);
        }
    }
    Ok(true)
}

/// Print where the change from `before` to `after` falls — the declarations of `before` it
/// touches, innermost first — and the one edit that makes it, or none; as JSON, or one line each.
pub fn diff(before: &Path, after: &Path, path: &str, as_json: bool) -> Result<bool, Failure> {
    let (old, new) = (files::read(before)?, files::read(after)?);
    let found = lotml_ide::diff::diff(&old, &new, path);
    let shown = |d: &Declaration| json!({"symbol": d.symbol, "kind": d.kind, "lines": d.lines});
    if as_json {
        let edit = found.edit.as_ref().map(|e| {
            let arguments: serde_json::Map<String, serde_json::Value> =
                e.arguments.iter().map(|(k, v)| ((*k).to_string(), json!(v))).collect();
            json!({"tool": e.tool, "arguments": arguments, "kind": e.kind()})
        });
        let declarations: Vec<_> = found.declarations.iter().map(shown).collect();
        println!("{}", json!({"declarations": declarations, "edit": edit}));
    } else {
        for d in &found.declarations {
            println!(
                "{} {} lines {}-{}",
                d.kind,
                d.symbol.as_deref().unwrap_or("(file start)"),
                d.lines[0],
                d.lines[1]
            );
        }
        match &found.edit {
            Some(e) => println!("edit: {} ({})", e.tool, e.kind()),
            None => println!("edit: none reproduces it"),
        }
    }
    Ok(true)
}

/// Print a file's symbol-addressed declarations as JSON: symbol, kind and lines, in source order.
pub fn outline(path: &Path) -> Result<bool, Failure> {
    let text = files::read(path)?;
    let listed: Vec<_> = lotml_ide::diff::declared(&text)
        .iter()
        .map(|d| json!({"symbol": d.symbol, "kind": d.kind, "lines": d.lines}))
        .collect();
    println!("{}", serde_json::Value::Array(listed));
    Ok(true)
}

/// Bytes of a failing block's run kept, as the `guide` tool keeps a candidate's.
const TEST_OUTPUT: usize = 4 * 1024 * 1024;

/// Print what the `guide` tool would make of an answer about `file`, shown to the guide as
/// `path`: whether it is within the answer schema, its locations' symbols in order, and its edit —
/// `none`, `passes`, or the reason the tool's gate withholds it, the failing block run in a
/// scratch directory with `deadline` seconds (specs/training-pipeline/ R4.2).
pub fn judge(file: &Path, answer: &Path, path: &str, failing: Option<&str>, deadline: u64) -> Result<bool, Failure> {
    let text = files::read(file)?;
    let content = files::read(answer)?;
    let Ok((read, _)) = guide::read_answer(&json!({"choices": [{"message": {"content": content}}]})) else {
        println!("{}", json!({"valid": false, "symbols": [], "edit": "none"}));
        return Ok(true);
    };
    let symbols: Vec<_> = read.locations.iter().map(|l| l.symbol.clone()).collect();
    let edit = match &read.edit {
        None => "none",
        Some(edit) if edit.path != path => "edit-fails-check",
        Some(edit) => {
            let clean = |new: &str| lotml_check::check_source(new).iter().all(|d| d.severity != Severity::Error);
            let run = |new: &str| failing.and_then(|block| runs(new, path, block, deadline));
            let candidate = failing.map(|_| (true, &run as &dyn Fn(&str) -> Option<bool>));
            guide::withheld(edit, &text, &clean, candidate).unwrap_or("passes")
        }
    };
    println!("{}", json!({"valid": true, "symbols": symbols, "edit": edit}));
    Ok(true)
}

/// Whether `block` passes with `text` as `path`'s file name, alone in a scratch directory; None
/// when it could not run.
fn runs(text: &str, path: &str, block: &str, deadline: u64) -> Option<bool> {
    let scratch = exec::Scratch::new().ok()?;
    std::fs::create_dir(scratch.0.join(".git")).ok()?;
    let target = scratch.0.join(Path::new(path).file_name()?);
    std::fs::write(&target, text).ok()?;
    let limits = exec::Limits { seconds: deadline, output: TEST_OUTPUT };
    let (_, report) = exec::test_report(std::slice::from_ref(&target), true, Some(&limits)).ok()?;
    let report: serde_json::Value = serde_json::from_str(report.trim()).ok()?;
    let edited = target.display().to_string();
    let row = report["tests"].as_array()?.iter().find(|r| r["file"] == edited.as_str() && r["name"] == block)?;
    Some(row["outcome"] == "pass")
}
