//! The LLVM backend: the counted IR of `lotml-ir` as textual LLVM IR, compiled by `clang` with
//! the runtime of `lotml-runtime` (adr:0021, specs/llvm-backend).

pub mod driver;
mod emit;

use std::path::Path;

use lotml_check::{Interfaces, check_resolved_with};
use lotml_diag::{Diagnostic, Severity};
use lotml_ir::lower;
use lotml_syntax::parse;

/// A compiled program: its LLVM IR, and the C libraries to link it with.
pub struct Program {
    pub ll: String,
    pub libraries: Vec<String>,
}

/// The LLVM IR of the program `source`, read from `path`; or the errors that stop it.
pub fn compile(source: &str, path: &Path) -> Result<String, Vec<Diagnostic>> {
    compile_program(source, path, &Interfaces::new(), false).map(|p| p.ll)
}

/// The program for `source`, importing the C libraries of `interfaces`; a Python module's import
/// is refused, and so, in this increment, are `test` blocks.
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
    let refused: Vec<Diagnostic> = lower::python_imports(&parsed.module)
        .iter()
        .map(|(module, span)| {
            Diagnostic::error(
                "E0401",
                *span,
                format!("`{module}` is a Python module, and a program built for the LLVM target runs without Python"),
            )
        })
        .collect();
    let mut lowered = match lower::lower(&parsed.module, &checked, source, tests) {
        Ok(lowered) => lowered,
        Err(errors) => return Err(refused.into_iter().chain(errors).collect()),
    };
    if !refused.is_empty() {
        return Err(refused);
    }
    if tests {
        let span = lotml_syntax::span::Span::new(0, 0);
        return Err(vec![Diagnostic::error(
            "E0402",
            span,
            "`--target llvm` does not run `test` blocks yet: run them with `--target python`".to_string(),
        )]);
    }
    lotml_ir::native(&mut lowered);
    let ll = emit::program(&lowered, &path.display().to_string())?;
    Ok(Program { ll, libraries: lowered.libraries.iter().cloned().collect() })
}
