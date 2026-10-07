//! `lotml dev`: commands that build the harness guide's data, hidden from help.

use std::path::Path;

use lotml_ide::diff::Declaration;
use lotml_ide::mutate::mutants;
use lotml_syntax::span::line_column;
use serde_json::json;

use crate::{Failure, files};

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
