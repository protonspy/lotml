//! The guide server's answer, read as untrusted input: into closed structs with capped strings and
//! checked line spans, its confidence from the log-probabilities of its tokens, and only the
//! locations that name the project's own files and declarations kept (specs/guide-tool/ R2.7,
//! R2.8).

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

/// The kinds of change an answer may name.
pub const KINDS: [&str; 7] = ["arm", "body", "definition", "add", "remove", "lines", "several"];
const PATH_LIMIT: usize = 512;
const SYMBOL_LIMIT: usize = 256;
const TEXT_LIMIT: usize = 16 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub path: String,
    /// None for a place before the file's first declaration.
    pub symbol: Option<String>,
    pub lines: [u32; 2],
}

/// One call of an edit tool, its arguments read field by field and written again from them, so
/// what was checked is what is shown.
#[derive(Clone, Debug, PartialEq)]
pub struct Edit {
    pub tool: String,
    pub path: String,
    pub arguments: Value,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Answer {
    pub locations: Vec<Location>,
    pub kind: String,
    pub edit: Option<Edit>,
}

/// A project file as the answer is checked against it: its line count and the symbols it
/// declares, named as `show` and `replace` take them.
pub struct ProjectFile {
    pub lines: usize,
    pub symbols: BTreeSet<String>,
}

const INVALID: &str = "invalid-answer";

/// The answer in the server's response and the confidence of its first location; the reason
/// `invalid-answer` when anything in it falls outside the schema.
pub fn read(response: &Value) -> Result<(Answer, f64), &'static str> {
    let choice = &response["choices"][0];
    let content = choice["message"]["content"].as_str().ok_or(INVALID)?;
    let parsed: Value = serde_json::from_str(content).map_err(|_| INVALID)?;
    let object = closed(&parsed, &["locations", "kind", "edit"], &["locations", "kind", "edit"])?;
    let listed = object["locations"].as_array().ok_or(INVALID)?;
    if listed.is_empty() || listed.len() > 3 {
        return Err(INVALID);
    }
    let locations = listed.iter().map(location).collect::<Result<Vec<_>, _>>()?;
    let kind = object["kind"].as_str().filter(|k| KINDS.contains(k)).ok_or(INVALID)?.to_string();
    let edit = match &object["edit"] {
        Value::Null => None,
        found => Some(edit(found)?),
    };
    Ok((Answer { locations, kind, edit }, confidence(content, &choice["logprobs"]["content"])))
}

/// The keys of `value`, an object: every one of `required` present, none outside `allowed`.
fn closed<'v>(value: &'v Value, required: &[&str], allowed: &[&str]) -> Result<&'v Map<String, Value>, &'static str> {
    let object = value.as_object().ok_or(INVALID)?;
    if object.keys().any(|k| !allowed.contains(&k.as_str())) || required.iter().any(|k| !object.contains_key(*k)) {
        return Err(INVALID);
    }
    Ok(object)
}

fn text(value: &Value, limit: usize) -> Result<&str, &'static str> {
    value.as_str().filter(|s| !s.is_empty() && s.len() <= limit).ok_or(INVALID)
}

fn location(value: &Value) -> Result<Location, &'static str> {
    let object = closed(value, &["path", "symbol", "lines"], &["path", "symbol", "lines"])?;
    let path = text(&object["path"], PATH_LIMIT)?.to_string();
    let symbol = match &object["symbol"] {
        Value::Null => None,
        found => Some(text(found, SYMBOL_LIMIT)?.to_string()),
    };
    let lines = object["lines"].as_array().ok_or(INVALID)?;
    let number = |v: &Value| v.as_u64().and_then(|n| u32::try_from(n).ok()).ok_or(INVALID);
    let [start, end] = lines.as_slice() else { return Err(INVALID) };
    let (start, end) = (number(start)?, number(end)?);
    if start == 0 || start > end {
        return Err(INVALID);
    }
    Ok(Location { path, symbol, lines: [start, end] })
}

/// An edit tool's call: the tool and its own arguments, each tool's fields and no others.
fn edit(value: &Value) -> Result<Edit, &'static str> {
    let object = closed(value, &["tool", "arguments"], &["tool", "arguments"])?;
    let tool = object["tool"].as_str().ok_or(INVALID)?;
    let arguments = &object["arguments"];
    let (required, allowed): (&[&str], &[&str]) = match tool {
        "replace" => (&["symbol", "text", "path"], &["symbol", "part", "arm", "text", "path"]),
        "add" => (&["text", "path"], &["text", "after", "path"]),
        "remove" => (&["symbol", "path"], &["symbol", "path"]),
        "edit" => (&["path", "search", "replace"], &["path", "search", "replace"]),
        _ => return Err(INVALID),
    };
    let found = closed(arguments, required, allowed)?;
    let mut written = Map::new();
    for (key, value) in found {
        let limit = match key.as_str() {
            "path" => PATH_LIMIT,
            "symbol" | "after" => SYMBOL_LIMIT,
            _ => TEXT_LIMIT,
        };
        let value = text(value, limit)?;
        written.insert(key.clone(), json!(value));
    }
    if tool == "replace" {
        let part = written.get("part").and_then(Value::as_str).unwrap_or("definition");
        if !["definition", "body", "arm"].contains(&part) || (part == "arm") != written.contains_key("arm") {
            return Err(INVALID);
        }
    }
    let path = written["path"].as_str().unwrap_or_default().to_string();
    Ok(Edit { tool: tool.to_string(), path, arguments: Value::Object(written) })
}

/// The probability the model gave its answer up to and including the first location's symbol:
/// the product of its tokens' probabilities. Without log-probabilities, none.
fn confidence(content: &str, tokens: &Value) -> f64 {
    let Some(tokens) = tokens.as_array().filter(|t| !t.is_empty()) else { return 0.0 };
    let end = first_symbol_end(content).unwrap_or(content.len());
    let (mut at, mut sum) = (0, 0.0);
    for token in tokens {
        if at >= end {
            break;
        }
        let Some(logprob) = token["logprob"].as_f64() else { return 0.0 };
        sum += logprob;
        at += token["token"].as_str().map_or(0, str::len);
    }
    sum.exp()
}

/// Where the value of the first location's `symbol` ends in the answer's text.
fn first_symbol_end(content: &str) -> Option<usize> {
    let locations = content.find("\"locations\"")?;
    let key = locations + content[locations..].find("\"symbol\"")? + "\"symbol\"".len();
    let rest = content[key..].trim_start().strip_prefix(':')?.trim_start();
    let start = content.len() - rest.len();
    if rest.starts_with("null") {
        return Some(start + 4);
    }
    let mut escaped = false;
    for (i, c) in rest.char_indices().skip(1) {
        match c {
            _ if escaped => escaped = false,
            '\\' => escaped = true,
            '"' => return Some(start + i + 1),
            _ => {}
        }
    }
    None
}

/// The locations whose path is exactly one of the project's files, whose symbol that file
/// declares and whose lines lie within it; the edit only when it targets a kept location's file.
pub fn keep(answer: Answer, files: &BTreeMap<String, ProjectFile>) -> Result<Answer, &'static str> {
    let locations: Vec<Location> = answer
        .locations
        .into_iter()
        .filter(|l| {
            files.get(&l.path).is_some_and(|f| {
                l.symbol.as_ref().is_none_or(|s| f.symbols.contains(s)) && l.lines[1] as usize <= f.lines
            })
        })
        .collect();
    if locations.is_empty() {
        return Err("no-location");
    }
    let edit = answer.edit.filter(|e| locations.iter().any(|l| l.path == e.path));
    Ok(Answer { locations, kind: answer.kind, edit })
}

/// The answer the tool may show: read, confident at `threshold`, and kept against the project.
pub fn judge(
    response: &Value,
    threshold: f64,
    files: &BTreeMap<String, ProjectFile>,
) -> Result<(Answer, f64), &'static str> {
    let (answer, confidence) = read(response)?;
    if confidence < threshold {
        return Err("not-confident");
    }
    Ok((keep(answer, files)?, confidence))
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use serde_json::{Value, json};

    use super::*;

    fn response(content: &str, tokens: &[(&str, f64)]) -> Value {
        let logprobs: Vec<Value> = tokens.iter().map(|(t, p)| json!({"token": t, "logprob": p})).collect();
        json!({"choices": [{"message": {"role": "assistant", "content": content}, "logprobs": {"content": logprobs}}]})
    }

    const CONTENT: &str = r#"{"locations": [{"path": "a.lotml", "symbol": "median", "lines": [3, 9]}], "kind": "body", "edit": {"tool": "replace", "arguments": {"symbol": "median", "part": "body", "text": "return None", "path": "a.lotml"}}}"#;

    /// `CONTENT` cut into tokens: the first location's symbol ends inside the third, so the
    /// confidence is the product of the first three.
    fn tokens() -> Vec<(&'static str, f64)> {
        let first = r#"{"locations": [{"path": "a.lotml", "#;
        let second = r#""symbol": "med"#;
        let third = r#"ian", "#;
        let rest = &CONTENT[first.len() + second.len() + third.len()..];
        vec![(first, -0.1), (second, -0.2), (third, -0.3), (Box::leak(rest.to_string().into_boxed_str()), -5.0)]
    }

    fn files() -> BTreeMap<String, ProjectFile> {
        let symbols: BTreeSet<String> = ["median", "mode", "Stats.mean"].iter().map(ToString::to_string).collect();
        BTreeMap::from([
            ("a.lotml".to_string(), ProjectFile { lines: 20, symbols }),
            ("b.lotml".to_string(), ProjectFile { lines: 5, symbols: BTreeSet::new() }),
        ])
    }

    #[test]
    fn an_answer_is_read_with_its_confidence_through_the_first_location_s_symbol() {
        let (answer, confidence) = read(&response(CONTENT, &tokens())).unwrap();
        assert_eq!(
            answer.locations,
            [Location { path: "a.lotml".into(), symbol: Some("median".into()), lines: [3, 9] }]
        );
        assert_eq!(answer.kind, "body");
        assert_eq!(answer.edit.as_ref().unwrap().tool, "replace");
        assert!((confidence - (-0.6f64).exp()).abs() < 1e-9, "{confidence}");
    }

    #[test]
    fn an_answer_without_log_probabilities_has_no_confidence() {
        let (_, confidence) = read(&json!({"choices": [{"message": {"content": CONTENT}}]})).unwrap();
        assert_eq!(confidence, 0.0);
    }

    #[test]
    fn an_answer_outside_the_schema_is_invalid() {
        let location = r#"{"path": "a.lotml", "symbol": null, "lines": [1, 1]}"#;
        let four = format!(
            r#"{{"locations": [{location}, {location}, {location}, {location}], "kind": "body", "edit": null}}"#
        );
        let long_path = format!(
            r#"{{"locations": [{{"path": "{}", "symbol": null, "lines": [1, 1]}}], "kind": "body", "edit": null}}"#,
            "a".repeat(513)
        );
        let long_text = format!(
            r#"{{"locations": [{location}], "kind": "body", "edit": {{"tool": "edit", "arguments": {{"path": "a.lotml", "search": "x", "replace": "{}"}}}}}}"#,
            "y".repeat(16 * 1024 + 1)
        );
        for content in [
            "not json".to_string(),
            r#"{"locations": [], "kind": "body", "edit": null}"#.to_string(),
            four,
            long_path,
            long_text,
            CONTENT.replace(r#""kind": "body""#, r#""kind": "rewrite""#),
            CONTENT.replace(r#""lines": [3, 9]"#, r#""lines": [9, 3]"#),
            CONTENT.replace(r#""lines": [3, 9]"#, r#""lines": [0, 3]"#),
            CONTENT.replace(r#""lines": [3, 9]"#, r#""lines": [3]"#),
            CONTENT.replace(r#""kind": "body""#, r#""kind": "body", "why": "x""#),
            CONTENT.replace(r#""path": "a.lotml", "symbol""#, r#""path": "a.lotml", "file": "x", "symbol""#),
            CONTENT.replace(r#""tool": "replace""#, r#""tool": "rename""#),
            CONTENT.replace(r#""part": "body", "#, r#""part": "body", "new_name": "x", "#),
            CONTENT.replace(r#", "path": "a.lotml"}}"#, "}}"),
            CONTENT.replace(r#""part": "body""#, r#""part": "header""#),
        ] {
            assert_eq!(read(&response(&content, &[])).unwrap_err(), "invalid-answer", "{content}");
        }
        assert_eq!(read(&json!({"error": "x"})).unwrap_err(), "invalid-answer");
    }

    #[test]
    fn every_edit_tool_s_arguments_are_read() {
        let edits = [
            r#"{"tool": "replace", "arguments": {"symbol": "median", "part": "arm", "arm": "Empty", "text": "x", "path": "a.lotml"}}"#,
            r#"{"tool": "add", "arguments": {"text": "fn g():\n    pass", "after": "median", "path": "a.lotml"}}"#,
            r#"{"tool": "add", "arguments": {"text": "fn g():\n    pass", "path": "a.lotml"}}"#,
            r#"{"tool": "remove", "arguments": {"symbol": "mode", "path": "a.lotml"}}"#,
            r#"{"tool": "edit", "arguments": {"path": "a.lotml", "search": "x", "replace": "y"}}"#,
        ];
        for edit in edits {
            let content = format!(
                r#"{{"locations": [{{"path": "a.lotml", "symbol": "median", "lines": [3, 9]}}], "kind": "lines", "edit": {edit}}}"#
            );
            let (answer, _) = read(&response(&content, &[])).unwrap_or_else(|e| panic!("{edit}: {e}"));
            assert_eq!(answer.edit.unwrap().path, "a.lotml");
        }
    }

    #[test]
    fn only_locations_in_the_project_s_own_files_and_declarations_are_kept() {
        let location = |path: &str, symbol: Option<&str>, lines: [u32; 2]| Location {
            path: path.into(),
            symbol: symbol.map(Into::into),
            lines,
        };
        let answer = Answer {
            locations: vec![
                location("./a.lotml", Some("median"), [1, 2]),
                location("a.lotml", Some("nowhere"), [1, 2]),
                location("a.lotml", Some("Stats.mean"), [4, 30]),
                location("b.lotml", None, [1, 5]),
            ],
            kind: "body".into(),
            edit: Some(Edit { tool: "edit".into(), path: "a.lotml".into(), arguments: json!({}) }),
        };
        let kept = keep(answer, &files()).unwrap();
        assert_eq!(kept.locations, [location("b.lotml", None, [1, 5])]);
        assert!(kept.edit.is_none(), "an edit on a file no kept location names is dropped");
        let none = Answer { locations: vec![location("/abs/a.lotml", None, [1, 1])], kind: "body".into(), edit: None };
        assert_eq!(keep(none, &files()).unwrap_err(), "no-location");
        let with_edit = Answer {
            locations: vec![location("a.lotml", Some("Stats.mean"), [2, 3])],
            kind: "body".into(),
            edit: Some(Edit { tool: "edit".into(), path: "a.lotml".into(), arguments: json!({}) }),
        };
        assert!(keep(with_edit, &files()).unwrap().edit.is_some());
    }

    #[test]
    fn below_the_threshold_the_guide_is_not_confident() {
        let low = response(CONTENT, &[(CONTENT, -2.0)]);
        assert_eq!(judge(&low, 0.5, &files()).unwrap_err(), "not-confident");
        let high = response(CONTENT, &[(CONTENT, -0.01)]);
        let (answer, confidence) = judge(&high, 0.5, &files()).unwrap();
        assert_eq!(answer.locations.len(), 1);
        assert!(confidence > 0.98);
    }
}
