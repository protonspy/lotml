//! The Python backend: a checked lotml program as a Python module (R13, R26; adr:0008).
//!
//! The module is a stub that hands the runtime the program as a Python syntax tree in JSON,
//! every node at its lotml position; the runtime compiles that tree under the `.lotml` file's
//! name, so a traceback names the lotml line and underlines the lotml expression. The checker's
//! types decide the rest: which arithmetic traps outside its integer type, which values are
//! copied — only those entering or leaving a `var` or an `inout` — and how a built-in method
//! whose lotml meaning differs from Python's is called.

mod emit;

use std::path::Path;

use lotml_diag::{Diagnostic, Severity};
use lotml_syntax::parse;

/// The runtime every generated module imports, shipped next to it as `lotml_rt.py`.
pub const RUNTIME: &str = include_str!("../runtime/lotml_rt.py");

/// The command that runs Python: `LOTML_PYTHON`, else the first of `python3`, `python` and the
/// Windows launcher `py -3` that answers `--version` — a name that only opens an app store,
/// as Windows ships `python.exe`, does not.
pub fn python() -> Option<Vec<String>> {
    if let Ok(path) = std::env::var("LOTML_PYTHON") {
        return Some(vec![path]);
    }
    let candidates: [&[&str]; 3] = [&["python3"], &["python"], &["py", "-3"]];
    candidates.into_iter().map(|c| c.iter().map(ToString::to_string).collect::<Vec<_>>()).find(|command| {
        std::process::Command::new(&command[0])
            .args(&command[1..])
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success() && !o.stdout.is_empty())
    })
}

/// The Python module for `source`, which was read from `path`; or the errors that stop it.
pub fn compile(source: &str, path: &Path) -> Result<String, Vec<Diagnostic>> {
    let parsed = parse(source);
    let mut diagnostics: Vec<Diagnostic> = parsed.errors.iter().map(lotml_check::syntax).collect();
    let (found, types) = lotml_check::check_typed(&parsed.module, source);
    diagnostics.extend(found);
    diagnostics.retain(|d| d.severity == Severity::Error);
    if !diagnostics.is_empty() {
        diagnostics.sort_by_key(|d| d.span.start);
        return Err(diagnostics);
    }
    let module = emit::module(source, &parsed.module, &types);
    let payload = serde_json::json!({"source": source, "module": module}).to_string();
    let file = path.display().to_string();
    Ok(format!(
        "# Compiled by lotml from {name}: edit that file, not this one.\nimport lotml_rt\n\nlotml_rt.load(globals(), {file}, {payload})\n",
        name = path.file_name().map_or(file.clone(), |n| n.to_string_lossy().into_owned()),
        file = serde_json::Value::String(file.clone()),
        payload = serde_json::Value::String(payload),
    ))
}
