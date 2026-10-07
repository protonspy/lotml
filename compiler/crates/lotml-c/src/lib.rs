//! The C backend: the counted IR of `lotml-ir` as one C file over the runtime of
//! `lotml-runtime` (R18, R19; adr:0016, specs/c-backend, specs/shared-ir).

pub mod driver;
mod emit;
mod types;

use lotml_ir::{ir as mir, lower};

use std::path::Path;

use lotml_check::{Interfaces, check_resolved_with};
use lotml_diag::{Diagnostic, Severity};
use lotml_syntax::parse;

/// The import of the Python module `path`, refused: the C target runs without Python (R5.3).
fn python_refused(path: &str, span: lotml_syntax::span::Span) -> Diagnostic {
    Diagnostic::error(
        "E0401",
        span,
        format!("`{path}` is a Python module, and a program built for the C target runs without Python"),
    )
}

/// A compiled program: its C, and the C libraries to link it with.
pub struct Program {
    pub c: String,
    pub libraries: Vec<String>,
}

/// The C program for `source`, which was read from `path`; or the errors that stop it.
pub fn compile(source: &str, path: &Path) -> Result<String, Vec<Diagnostic>> {
    compile_program(source, path, &Interfaces::new(), false).map(|p| p.c)
}

/// The C program running the `test` blocks of `source`, each under a catcher, which ends by
/// writing what `lotml test` reports as one line of JSON (R1.4).
pub fn compile_tests(source: &str, path: &Path) -> Result<String, Vec<Diagnostic>> {
    compile_program(source, path, &Interfaces::new(), true).map(|p| p.c)
}

/// The program for `source`, or with `tests` the program running its `test` blocks, importing
/// the C libraries of `interfaces`; a Python module's import is refused (R5.3).
pub fn compile_program(
    source: &str,
    path: &Path,
    interfaces: &Interfaces,
    tests: bool,
) -> Result<Program, Vec<Diagnostic>> {
    let parsed = parse(source);
    let mut errors: Vec<Diagnostic> = parsed.errors.iter().map(lotml_check::syntax).collect();
    let checked = check_resolved_with(&parsed.module, source, interfaces);
    errors.extend(checked.diagnostics.iter().filter(|d| d.severity == Severity::Error).cloned());
    if !errors.is_empty() {
        return Err(errors);
    }
    let mut lowered = lower::lower(&parsed.module, &checked, source, tests)?;
    if !lowered.python_imports.is_empty() {
        return Err(lowered.python_imports.iter().map(|(path, span)| python_refused(path, *span)).collect());
    }
    lotml_ir::native(&mut lowered);
    let c = emit::program(&lowered, &path.display().to_string(), tests);
    Ok(Program { c, libraries: lowered.libraries.iter().filter(|l| !driver::linked_always(l)).cloned().collect() })
}
