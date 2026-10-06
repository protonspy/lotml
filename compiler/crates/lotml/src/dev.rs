//! `lotml dev`: commands that build the harness guide's data, hidden from help.

use std::path::Path;

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
