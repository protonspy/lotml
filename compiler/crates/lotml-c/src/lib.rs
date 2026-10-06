//! The C backend: a checked lotml program as one C file over a counting runtime (R18, R19;
//! adr:0014, specs/c-backend).

pub mod driver;
mod emit;
mod lower;
mod mir;

use std::path::Path;

use lotml_check::check_resolved;
use lotml_diag::{Diagnostic, Severity};
use lotml_syntax::parse;

/// The runtime's declarations, written next to every compiled program as `lotml.h`.
pub const RUNTIME_H: &str = include_str!("../runtime/lotml.h");

/// The runtime's functions, written as `lotml.c` and included into the program, which is one
/// translation unit.
pub const RUNTIME_C: &str = include_str!("../runtime/lotml.c");

/// Write the runtime into `dir`, where a compiled program includes it from.
pub fn write_runtime(dir: &Path) -> std::io::Result<()> {
    std::fs::write(dir.join("lotml.h"), RUNTIME_H)?;
    std::fs::write(dir.join("lotml.c"), RUNTIME_C)
}

/// The C program for `source`, which was read from `path`; or the errors that stop it.
pub fn compile(source: &str, path: &Path) -> Result<String, Vec<Diagnostic>> {
    let parsed = parse(source);
    let mut errors: Vec<Diagnostic> = parsed.errors.iter().map(lotml_check::syntax).collect();
    let checked = check_resolved(&parsed.module, source);
    errors.extend(checked.diagnostics.iter().filter(|d| d.severity == Severity::Error).cloned());
    if !errors.is_empty() {
        return Err(errors);
    }
    let lowered = lower::lower(&parsed.module, &checked, source)?;
    Ok(emit::program(&lowered, &path.display().to_string()))
}
