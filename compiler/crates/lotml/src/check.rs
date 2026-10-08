//! `lotml check`: diagnostics root cause first and bounded, safe fixes applied on request, only
//! what an edit introduced since a revision, or a verdict on a file still being written.

use std::collections::BTreeMap;
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
    // Every interface a checked file may import, so a broken one is reported once.
    let mut interfaces: BTreeMap<String, (String, String)> = BTreeMap::new();
    for path in &paths {
        let text = files::read(path)?;
        let bindings = files::interfaces_for(path, &text);
        for b in &bindings {
            if let Some(file) = &b.path {
                interfaces.entry(file.display().to_string()).or_insert_with(|| (b.module.clone(), b.text.clone()));
            }
        }
        let bindings = bindings.into_iter().map(|b| (b.module, b.text)).collect();
        sources.push(SourceFile::create(&db, path.display().to_string(), text, bindings));
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
            *diagnostics = lotml_diag::introduced(std::mem::take(diagnostics), file.text(&db), &before, &old);
        }
    }
    let mut reports: Vec<Report> = sources
        .iter()
        .zip(found)
        .map(|(&f, diagnostics)| Report { file: f.path(&db), text: f.text(&db), diagnostics })
        .collect();
    for (file, (module, text)) in &interfaces {
        let problems = lotml_check::interface_of(module, text).1;
        if !problems.is_empty() && options.since.is_none() {
            reports.push(Report { file, text, diagnostics: problems });
        }
    }
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

fn prefix(db: &Database, sources: &[SourceFile], format: Format) -> Result<bool, Failure> {
    let mut complete = true;
    let mut rows = Vec::new();
    let mut text = String::new();
    for &file in sources {
        let found = lotml_check::check_prefix_with(file.text(db), lotml_db::interfaces(db, file));
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
