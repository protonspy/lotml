//! `lotml check`: diagnostics root cause first and bounded, safe fixes applied on request, only
//! what an edit introduced since a revision, or a verdict on a file still being written.

use std::collections::HashMap;
use std::path::PathBuf;

use lotml_db::{Database, SourceFile, diagnostics};
use lotml_diag::{DEFAULT_LIMIT, Diagnostic, Report, Severity};
use salsa::Setter;

use crate::{Failure, Format, files};

pub struct Options {
    pub paths: Vec<PathBuf>,
    pub format: Format,
    pub all: bool,
    pub fix: bool,
    pub since: Option<String>,
    pub prefix: bool,
}

/// How many rounds of fixes to apply: a fix can expose another, but not forever.
const FIX_ROUNDS: usize = 3;

pub fn run(options: &Options) -> Result<bool, Failure> {
    let paths = files::expand(&options.paths)?;
    let mut db = Database::default();
    let mut sources = Vec::new();
    for path in &paths {
        let text = files::read(path)?;
        sources.push(SourceFile::new(&db, path.display().to_string(), text));
    }
    if options.prefix {
        return prefix(&db, &sources, options.format);
    }
    let mut fixed = 0;
    if options.fix {
        for (path, &file) in paths.iter().zip(&sources) {
            let before = file.text(&db).clone();
            for _ in 0..FIX_ROUNDS {
                let (text, applied) = lotml_diag::apply_fixes(file.text(&db), diagnostics(&db, file));
                if applied == 0 {
                    break;
                }
                fixed += applied;
                file.set_text(&mut db).to(text);
            }
            if *file.text(&db) != before {
                std::fs::write(path, file.text(&db))
                    .map_err(|e| Failure(format!("cannot write {}: {e}", path.display())))?;
            }
        }
    }
    let mut found: Vec<Vec<Diagnostic>> = sources.iter().map(|&f| diagnostics(&db, f).clone()).collect();
    if let Some(rev) = &options.since {
        for ((path, &file), diagnostics) in paths.iter().zip(&sources).zip(found.iter_mut()) {
            let old = files::at_revision(rev, path)?;
            let before = lotml_check::check_source(&old);
            *diagnostics = introduced(std::mem::take(diagnostics), file.text(&db), &before, &old);
        }
    }
    let reports: Vec<Report> = sources
        .iter()
        .zip(found)
        .map(|(&f, diagnostics)| Report { file: f.path(&db), text: f.text(&db), diagnostics })
        .collect();
    let limit = if options.all { None } else { Some(DEFAULT_LIMIT) };
    match options.format {
        Format::Text => {
            print!("{}", lotml_diag::text(&reports, limit));
            if fixed > 0 {
                println!("applied {fixed} fix{}", if fixed == 1 { "" } else { "es" });
            }
            if let Some(rev) = &options.since
                && reports.iter().all(|r| r.diagnostics.is_empty())
            {
                println!("no errors introduced since {rev}");
            }
        }
        Format::Json => {
            let mut out = lotml_diag::json(&reports, limit);
            out["summary"]["fixed"] = fixed.into();
            if let Some(rev) = &options.since {
                out["summary"]["since"] = rev.clone().into();
            }
            println!("{out}");
        }
        Format::Sarif => println!("{}", lotml_diag::sarif(&reports)),
    }
    Ok(reports.iter().all(|r| r.diagnostics.iter().all(|d| d.severity != Severity::Error)))
}

/// The diagnostics of `after` that `before` did not have. A diagnostic is matched by its code,
/// its message and the text of its line, so moving code does not make it new.
fn introduced(after: Vec<Diagnostic>, after_text: &str, before: &[Diagnostic], before_text: &str) -> Vec<Diagnostic> {
    let key = |d: &Diagnostic, text: &str| {
        let start = (d.span.start as usize).min(text.len());
        let line_start = text[..start].rfind('\n').map_or(0, |i| i + 1);
        let line_end = text[start..].find('\n').map_or(text.len(), |i| start + i);
        (d.code, d.message.clone(), text[line_start..line_end].trim().to_string())
    };
    let mut old: HashMap<_, usize> = HashMap::new();
    for d in before {
        *old.entry(key(d, before_text)).or_default() += 1;
    }
    after
        .into_iter()
        .filter(|d| match old.get_mut(&key(d, after_text)) {
            Some(n) if *n > 0 => {
                *n -= 1;
                false
            }
            _ => true,
        })
        .collect()
}

fn prefix(db: &Database, sources: &[SourceFile], format: Format) -> Result<bool, Failure> {
    let mut complete = true;
    let mut rows = Vec::new();
    let mut text = String::new();
    for &file in sources {
        let found = lotml_check::check_prefix(file.text(db));
        complete &= found.verdict != lotml_check::Verdict::Error;
        let report = Report { file: file.path(db), text: file.text(db), diagnostics: found.errors };
        match format {
            Format::Text => {
                let held = if found.held.is_empty() { String::new() } else { format!(" ({} held)", found.held.len()) };
                text += &format!("{}: {}{held}\n", file.path(db), found.verdict.as_str());
                if !report.diagnostics.is_empty() {
                    text += &report.text(Some(DEFAULT_LIMIT));
                }
            }
            _ => rows.push(serde_json::json!({
                "file": file.path(db),
                "verdict": found.verdict.as_str(),
                "held": found.held.len(),
                "diagnostics": report.json(Some(DEFAULT_LIMIT))["diagnostics"],
            })),
        }
    }
    match format {
        Format::Text => print!("{text}"),
        _ => println!("{}", serde_json::json!({"version": 1, "prefix": rows})),
    }
    Ok(complete)
}

#[cfg(test)]
mod tests {
    use lotml_syntax::span::Span;

    use super::*;

    #[test]
    fn only_new_diagnostics_are_introduced() {
        let before_text = "a\nb\n";
        let after_text = "z\na\nb\nc\n";
        let old = vec![Diagnostic::error("E0201", Span::new(0, 1), "`a` is not defined")];
        let new = vec![
            Diagnostic::error("E0201", Span::new(2, 3), "`a` is not defined"),
            Diagnostic::error("E0201", Span::new(6, 7), "`c` is not defined"),
        ];
        let kept = introduced(new, after_text, &old, before_text);
        assert_eq!(kept.iter().map(|d| d.message.as_str()).collect::<Vec<_>>(), vec!["`c` is not defined"]);
    }
}
