//! Diagnostics: what the compiler says, in a form an agent can act on.
//!
//! Every diagnostic has a stable code with an explanation page, a primary span, secondary
//! spans with labels, the admissible alternatives (the names, methods, variants or types in
//! scope that would have fitted) and fixes with an applicability level — `MachineApplicable`
//! fixes are safe to apply without a model round. The JSON form is versioned and only ever
//! gains fields.

use lotml_syntax::span::{Span, line_column};
use serde::Serialize;

pub mod codes;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum Applicability {
    /// Certainly what was meant: `check --fix` applies it.
    MachineApplicable,
    /// Probably what was meant; a person or a model decides.
    MaybeIncorrect,
    /// A template with placeholders to fill in.
    HasPlaceholders,
    Unspecified,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Edit {
    #[serde(skip)]
    pub span: Span,
    pub replacement: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Fix {
    pub message: String,
    pub applicability: Applicability,
    pub edits: Vec<Edit>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Label {
    #[serde(skip)]
    pub span: Span,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Diagnostic {
    pub code: &'static str,
    pub severity: Severity,
    pub message: String,
    #[serde(skip)]
    pub span: Span,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
    /// What fits here: names in scope, methods of the type, variants not yet matched…
    pub alternatives: Vec<String>,
    pub fixes: Vec<Fix>,
}

impl Diagnostic {
    pub fn error(code: &'static str, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic {
            code,
            severity: Severity::Error,
            message: message.into(),
            span,
            labels: vec![],
            notes: vec![],
            alternatives: vec![],
            fixes: vec![],
        }
    }

    pub fn warning(code: &'static str, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic { severity: Severity::Warning, ..Diagnostic::error(code, span, message) }
    }

    pub fn label(mut self, span: Span, message: impl Into<String>) -> Diagnostic {
        self.labels.push(Label { span, message: message.into() });
        self
    }

    pub fn note(mut self, note: impl Into<String>) -> Diagnostic {
        self.notes.push(note.into());
        self
    }

    pub fn alternatives(mut self, names: impl IntoIterator<Item = String>) -> Diagnostic {
        self.alternatives.extend(names);
        self
    }

    pub fn fix(
        mut self,
        message: impl Into<String>,
        applicability: Applicability,
        edits: Vec<(Span, String)>,
    ) -> Diagnostic {
        let edits = edits.into_iter().map(|(span, replacement)| Edit { span, replacement }).collect();
        self.fixes.push(Fix { message: message.into(), applicability, edits });
        self
    }
}

/// A position in a file, as the JSON shows it: byte offsets, and lines and columns from 1.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Location {
    pub start: u32,
    pub end: u32,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

impl Location {
    pub fn new(text: &str, span: Span) -> Location {
        let (line, column) = line_column(text, span.start);
        let (end_line, end_column) = line_column(text, span.end);
        Location { start: span.start, end: span.end, line, column, end_line, end_column }
    }
}

#[derive(Serialize)]
struct LabelOut<'a> {
    location: Location,
    message: &'a str,
}

#[derive(Serialize)]
struct EditOut<'a> {
    location: Location,
    replacement: &'a str,
}

#[derive(Serialize)]
struct FixOut<'a> {
    message: &'a str,
    applicability: Applicability,
    edits: Vec<EditOut<'a>>,
}

#[derive(Serialize)]
struct DiagnosticOut<'a> {
    code: &'static str,
    severity: Severity,
    message: &'a str,
    file: &'a str,
    location: Location,
    labels: Vec<LabelOut<'a>>,
    notes: &'a [String],
    alternatives: &'a [String],
    fixes: Vec<FixOut<'a>>,
    explanation: String,
}

/// One file's diagnostics, ready to report.
pub struct Report<'a> {
    pub file: &'a str,
    pub text: &'a str,
    pub diagnostics: Vec<Diagnostic>,
}

/// How many diagnostics a report shows unless asked for all: long reports hurt repair.
pub const DEFAULT_LIMIT: usize = 5;

/// How early in the chain of causes a code sits: a syntax error can cause anything after it,
/// an unknown name a type error, a type error nothing about mutability.
fn cause(code: &str) -> u8 {
    match code {
        c if c.starts_with("E00") || c.starts_with("E01") => 0,
        "E0201" | "E0202" | "E0210" | "E0216" | "E0217" => 1,
        c if c.starts_with("E02") => 2,
        _ => 3,
    }
}

impl Report<'_> {
    /// Root causes first — errors before warnings, syntax before names before types before
    /// mutability — then by position; at most `limit` of them.
    pub fn shown(&self, limit: Option<usize>) -> Vec<&Diagnostic> {
        shown(std::slice::from_ref(self), limit).into_iter().map(|(_, d)| d).collect()
    }

    fn out<'b>(&'b self, d: &'b Diagnostic) -> DiagnosticOut<'b> {
        DiagnosticOut {
            code: d.code,
            severity: d.severity,
            message: &d.message,
            file: self.file,
            location: Location::new(self.text, d.span),
            labels: d
                .labels
                .iter()
                .map(|l| LabelOut { location: Location::new(self.text, l.span), message: &l.message })
                .collect(),
            notes: &d.notes,
            alternatives: &d.alternatives,
            fixes: d
                .fixes
                .iter()
                .map(|f| FixOut {
                    message: &f.message,
                    applicability: f.applicability,
                    edits: f
                        .edits
                        .iter()
                        .map(|e| EditOut { location: Location::new(self.text, e.span), replacement: &e.replacement })
                        .collect(),
                })
                .collect(),
            explanation: format!("lotml explain {}", d.code),
        }
    }

    /// The versioned JSON form: `{"version": 1, "diagnostics": […], "summary": {…}}`.
    pub fn json(&self, limit: Option<usize>) -> serde_json::Value {
        json(std::slice::from_ref(self), limit)
    }

    /// A human report: `file:line:column: error[E0201]: message`, then the line and labels.
    pub fn text(&self, limit: Option<usize>) -> String {
        text(std::slice::from_ref(self), limit)
    }

    /// SARIF 2.1.0, for tools that read it.
    pub fn sarif(&self) -> serde_json::Value {
        sarif(std::slice::from_ref(self))
    }

    fn text_of(&self, d: &Diagnostic) -> String {
        let mut out = String::new();
        let at = Location::new(self.text, d.span);
        let severity = match d.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
        };
        out += &format!("{}:{}:{}: {severity}[{}]: {}\n", self.file, at.line, at.column, d.code, d.message);
        if let Some(line) = self.text.lines().nth(at.line - 1) {
            let width = if at.end_line == at.line { at.end_column.saturating_sub(at.column).max(1) } else { 1 };
            out += &format!("    {line}\n    {}{}\n", " ".repeat(at.column - 1), "^".repeat(width));
        }
        for label in &d.labels {
            let at = Location::new(self.text, label.span);
            out += &format!("  {}:{}: {}\n", at.line, at.column, label.message);
        }
        if !d.alternatives.is_empty() {
            out += &format!("  alternatives: {}\n", d.alternatives.join(", "));
        }
        for fix in &d.fixes {
            out += &format!("  fix: {}\n", fix.message);
        }
        for note in &d.notes {
            out += &format!("  note: {note}\n");
        }
        out
    }
}

/// The diagnostics of several files, root causes first across all of them, each with the index
/// of its report; at most `limit`.
pub fn shown<'a>(reports: &'a [Report<'a>], limit: Option<usize>) -> Vec<(usize, &'a Diagnostic)> {
    let mut all: Vec<(usize, &Diagnostic)> =
        reports.iter().enumerate().flat_map(|(i, r)| r.diagnostics.iter().map(move |d| (i, d))).collect();
    all.sort_by_key(|(i, d)| (d.severity, cause(d.code), *i, d.span.start));
    all.truncate(limit.unwrap_or(usize::MAX));
    all
}

/// The versioned JSON form of several files' diagnostics.
pub fn json(reports: &[Report], limit: Option<usize>) -> serde_json::Value {
    let shown = shown(reports, limit);
    let total: usize = reports.iter().map(|r| r.diagnostics.len()).sum();
    let errors: usize =
        reports.iter().map(|r| r.diagnostics.iter().filter(|d| d.severity == Severity::Error).count()).sum();
    serde_json::json!({
        "version": 1,
        "diagnostics": shown.iter().map(|(i, d)| reports[*i].out(d)).collect::<Vec<_>>(),
        "summary": {
            "files": reports.len(),
            "errors": errors,
            "warnings": total - errors,
            "shown": shown.len(),
            "hidden": total - shown.len(),
            "clean": total == 0,
        },
    })
}

/// The human form of several files' diagnostics, saying so when there are none.
pub fn text(reports: &[Report], limit: Option<usize>) -> String {
    let shown = shown(reports, limit);
    let mut out: String = shown.iter().map(|(i, d)| reports[*i].text_of(d)).collect();
    let total: usize = reports.iter().map(|r| r.diagnostics.len()).sum();
    if total > shown.len() {
        out += &format!("{} more; `--all` shows every one\n", total - shown.len());
    }
    if total == 0 {
        match reports {
            [one] => out += &format!("{}: no errors\n", one.file),
            _ => out += &format!("{} files: no errors\n", reports.len()),
        }
    }
    out
}

/// SARIF 2.1.0 for several files.
pub fn sarif(reports: &[Report]) -> serde_json::Value {
    let results: Vec<_> = reports
        .iter()
        .flat_map(|r| {
            r.diagnostics.iter().map(move |d| {
                let at = Location::new(r.text, d.span);
                serde_json::json!({
                    "ruleId": d.code,
                    "level": match d.severity { Severity::Error => "error", Severity::Warning => "warning" },
                    "message": {"text": d.message},
                    "locations": [{"physicalLocation": {
                        "artifactLocation": {"uri": r.file},
                        "region": {
                            "startLine": at.line, "startColumn": at.column,
                            "endLine": at.end_line, "endColumn": at.end_column,
                            "byteOffset": at.start, "byteLength": at.end - at.start,
                        },
                    }}],
                })
            })
        })
        .collect();
    serde_json::json!({
        "version": "2.1.0",
        "$schema": "https://json.schemastore.org/sarif-2.1.0.json",
        "runs": [{
            "tool": {"driver": {
                "name": "lotml",
                "rules": codes::ALL.iter().map(|c| serde_json::json!({"id": c.code, "shortDescription": {"text": c.title}})).collect::<Vec<_>>(),
            }},
            "results": results,
        }],
    })
}

/// Apply the `MachineApplicable` fixes of `diagnostics` to `text`; edits that overlap an
/// earlier one are skipped. Returns the new text and how many fixes were applied.
pub fn apply_fixes(text: &str, diagnostics: &[Diagnostic]) -> (String, usize) {
    let mut fixes: Vec<&Fix> = diagnostics
        .iter()
        .flat_map(|d| d.fixes.iter().take(1))
        .filter(|f| f.applicability == Applicability::MachineApplicable)
        .collect();
    fixes.sort_by_key(|f| f.edits.first().map_or(0, |e| e.span.start));
    let mut out = String::with_capacity(text.len());
    let mut at = 0usize;
    let mut applied = 0;
    for fix in fixes {
        if fix.edits.iter().any(|e| (e.span.start as usize) < at) {
            continue;
        }
        let mut edits: Vec<&Edit> = fix.edits.iter().collect();
        edits.sort_by_key(|e| e.span.start);
        for edit in edits {
            out.push_str(&text[at..edit.span.start as usize]);
            out.push_str(&edit.replacement);
            at = edit.span.end as usize;
        }
        applied += 1;
    }
    out.push_str(&text[at..]);
    (out, applied)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Report<'static> {
        let text = "fn f():\n    retrun 1\n";
        let d = Diagnostic::error("E0201", Span::new(12, 18), "`retrun` is not defined")
            .alternatives(["return".to_string()])
            .fix("write `return`", Applicability::MachineApplicable, vec![(Span::new(12, 18), "return".into())]);
        Report { file: "f.lotml", text, diagnostics: vec![d] }
    }

    #[test]
    fn json_has_codes_locations_alternatives_and_fixes() {
        let json = sample().json(None);
        let d = &json["diagnostics"][0];
        assert_eq!(d["code"], "E0201");
        assert_eq!(d["location"]["line"], 2);
        assert_eq!(d["location"]["column"], 5);
        assert_eq!(d["alternatives"][0], "return");
        assert_eq!(d["fixes"][0]["applicability"], "MachineApplicable");
        assert_eq!(d["explanation"], "lotml explain E0201");
        assert_eq!(json["summary"]["errors"], 1);
    }

    #[test]
    fn reports_are_bounded_and_say_how_many_are_hidden() {
        let mut report = sample();
        let first = report.diagnostics[0].clone();
        for _ in 0..7 {
            report.diagnostics.push(first.clone());
        }
        assert_eq!(report.json(Some(DEFAULT_LIMIT))["summary"]["hidden"], 3);
        assert!(report.text(Some(DEFAULT_LIMIT)).contains("3 more"));
    }

    #[test]
    fn root_causes_come_first() {
        let text = "fn f():\n    x = y + 1\n";
        let mismatch = Diagnostic::error("E0204", Span::new(16, 21), "mismatch");
        let unknown = Diagnostic::error("E0201", Span::new(16, 17), "`y` is not defined");
        let syntax = Diagnostic::error("E0003", Span::new(20, 21), "unexpected token");
        let warning = Diagnostic::warning("E0308", Span::new(0, 1), "dropped");
        let report = Report { file: "f.lotml", text, diagnostics: vec![warning, mismatch, unknown, syntax] };
        let order: Vec<&str> = report.shown(None).iter().map(|d| d.code).collect();
        assert_eq!(order, vec!["E0003", "E0201", "E0204", "E0308"]);
    }

    #[test]
    fn several_files_share_one_bound_and_one_ranking() {
        let a = Report {
            file: "a.lotml",
            text: "x\n",
            diagnostics: vec![Diagnostic::error("E0204", Span::new(0, 1), "m")],
        };
        let b = Report {
            file: "b.lotml",
            text: "y\n",
            diagnostics: vec![Diagnostic::error("E0201", Span::new(0, 1), "n")],
        };
        let c = Report { file: "c.lotml", text: "", diagnostics: vec![] };
        let reports = [a, b, c];
        let out = json(&reports, Some(1));
        assert_eq!(out["diagnostics"][0]["file"], "b.lotml");
        assert_eq!(out["summary"]["files"], 3);
        assert_eq!(out["summary"]["hidden"], 1);
        assert!(text(&reports[2..], None).contains("c.lotml: no errors"));
        assert_eq!(sarif(&reports)["runs"][0]["results"].as_array().map(Vec::len), Some(2));
    }

    #[test]
    fn a_clean_file_says_so() {
        let report = Report { file: "f.lotml", text: "", diagnostics: vec![] };
        assert_eq!(report.json(None)["summary"]["clean"], true);
        assert!(report.text(None).contains("no errors"));
    }

    #[test]
    fn machine_applicable_fixes_apply_and_others_do_not() {
        let report = sample();
        assert_eq!(apply_fixes(report.text, &report.diagnostics), ("fn f():\n    return 1\n".into(), 1));
        let mut maybe = report.diagnostics.clone();
        maybe[0].fixes[0].applicability = Applicability::MaybeIncorrect;
        assert_eq!(apply_fixes(report.text, &maybe).1, 0);
    }

    #[test]
    fn sarif_names_rules_and_regions() {
        let sarif = sample().sarif();
        assert_eq!(sarif["version"], "2.1.0");
        assert_eq!(sarif["runs"][0]["results"][0]["ruleId"], "E0201");
        assert_eq!(sarif["runs"][0]["results"][0]["locations"][0]["physicalLocation"]["region"]["startLine"], 2);
    }

    #[test]
    fn every_code_is_unique_and_explained() {
        let mut seen = std::collections::HashSet::new();
        for code in codes::ALL {
            assert!(seen.insert(code.code), "{} twice", code.code);
            assert!(!code.explanation.trim().is_empty(), "{} unexplained", code.code);
        }
    }
}
