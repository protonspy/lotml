//! The Python backend: a checked lotml program as a Python module (R13, R26; adr:0008).
//!
//! The module is a stub that hands the runtime the program as a Python syntax tree in JSON,
//! every node at its lotml position; the runtime compiles that tree under the `.lotml` file's
//! name, so a traceback names the lotml line and underlines the lotml expression. The checker's
//! types decide the rest: which arithmetic traps outside its integer type, which values are
//! copied — only those entering or leaving a `var` or an `inout` — and how a built-in method
//! whose lotml meaning differs from Python's is called.

mod boundary;
mod emit;

use std::path::Path;

use lotml_check::{Checked, Interfaces};
use lotml_diag::{Diagnostic, Severity};
use lotml_syntax::parse;

pub use boundary::{descriptor, stub};

/// A compiled module: the Python that runs it, and what the checker found, from which its
/// `.pyi` is written.
pub struct Compiled {
    pub module: String,
    pub checked: Checked,
}

/// The runtime every generated module imports, shipped next to it as `lotml_rt.py`.
pub const RUNTIME: &str = include_str!("../runtime/lotml_rt.py");

/// The script `lotml bind` runs: a stub read with Python's parser, written as a lotml interface.
pub const BIND: &str = include_str!("../runtime/lotml_bind.py");

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
    compile_with(source, path, &Interfaces::new()).map(|c| c.module)
}

/// [`compile`] for a module that may import the Python modules in `interfaces`. The module
/// loads through `lotml_rt.load_module`, which shows Python its functions behind the checked
/// boundary (adr:0012).
pub fn compile_with(source: &str, path: &Path, interfaces: &Interfaces) -> Result<Compiled, Vec<Diagnostic>> {
    let parsed = parse(source);
    let mut diagnostics: Vec<Diagnostic> = parsed.errors.iter().map(lotml_check::syntax).collect();
    let checked = lotml_check::check_resolved_with(&parsed.module, source, interfaces);
    diagnostics.extend(checked.diagnostics.iter().cloned());
    diagnostics.retain(|d| d.severity == Severity::Error);
    if !diagnostics.is_empty() {
        diagnostics.sort_by_key(|d| d.span.start);
        return Err(diagnostics);
    }
    let module = emit::module(source, &parsed.module, &checked.types, &checked.foreign);
    let exports = boundary::exports(&checked);
    let payload = serde_json::json!({"source": source, "module": module, "exports": exports}).to_string();
    let file = path.display().to_string();
    // The path goes in as a JSON string, never into the comment: a file name may hold a newline,
    // which in a comment would start a line of Python.
    let module = format!(
        "# Compiled by lotml: edit the .lotml source, not this file.\nimport lotml_rt\n\nlotml_rt.load_module(globals(), {file}, {payload})\n",
        file = serde_json::Value::String(file),
        payload = serde_json::Value::String(payload),
    );
    Ok(Compiled { module, checked })
}
