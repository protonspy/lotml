//! The LLVM backend: the counted IR of `lotml-ir` as textual LLVM IR, compiled by `clang` with
//! the runtime of `lotml-runtime` (adr:0021, specs/llvm-backend); a module as a program, or as a
//! library C calls (specs/c-abi-export).

pub mod cache;
pub mod driver;
mod emit;
pub mod export;
mod module;
mod sha256;
mod types;

#[cfg(test)]
mod layout;

use std::path::Path;

use lotml_check::ty::Ty;
use lotml_check::{Checked, Interfaces, check_resolved_with};
use lotml_diag::{Diagnostic, Severity};
use lotml_ir::lower::{self, Lowered};
use lotml_syntax::ast::Module;
use lotml_syntax::parse;
use lotml_syntax::span::Span;

use crate::emit::Entry;

/// A compiled program: its LLVM IR, and the C libraries to link it with.
pub struct Program {
    pub ll: String,
    pub libraries: Vec<String>,
}

/// A compiled library: its LLVM IR, the header declaring what it exports, the C libraries to link
/// it with, and a warning for each function left out of it.
pub struct Library {
    pub ll: String,
    pub header: String,
    pub libraries: Vec<String>,
    pub warnings: Vec<Diagnostic>,
}

/// The LLVM IR of the program `source`, read from `path`; or the errors that stop it.
pub fn compile(source: &str, path: &Path) -> Result<String, Vec<Diagnostic>> {
    compile_program(source, path, &Interfaces::new(), false, false).map(|p| p.ll)
}

/// `source` parsed, checked and lowered with the native passes, `inspect` seeing the module and
/// its checked signatures before the lowering; or the errors that stop it. A Python module's
/// import is refused: what the LLVM target builds runs without Python.
fn lowered<T>(
    source: &str,
    interfaces: &Interfaces,
    tests: bool,
    inspect: impl FnOnce(&Module, &Checked) -> Result<T, Vec<Diagnostic>>,
) -> Result<(Lowered, T), Vec<Diagnostic>> {
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
    let inspected = inspect(&parsed.module, &checked);
    let lowered = match lower::lower(&parsed.module, &checked, source, tests) {
        Ok(lowered) => lowered,
        Err(errors) => return Err(refused.into_iter().chain(errors).collect()),
    };
    if !refused.is_empty() {
        return Err(refused);
    }
    // A `PyObject` is a Python value, which a native program has no interpreter to hold
    // (specs/python-object R3.1); a program importing Python is refused above, at the import.
    let holding: Vec<Diagnostic> = lowered
        .functions
        .iter()
        .filter(|f| f.locals.iter().map(|l| &l.ty).chain([&f.ret]).any(Ty::holds_py_object))
        .map(|f| {
            Diagnostic::error("E0402", f.span, format!("`{}` holds a `PyObject`, a Python value", f.source_name))
                .note("a program holding a Python value runs on the Python target: `lotml run`")
        })
        .collect();
    if !holding.is_empty() {
        return Err(holding);
    }
    let inspected = inspected?;
    let mut lowered = lotml_ir::mono::mono(lowered)?;
    lotml_ir::native(&mut lowered);
    Ok((lowered, inspected))
}

/// The program for `source`, importing the C libraries of `interfaces`. With `tests`, it runs the
/// `test` blocks; with `lines`, it carries line tables that name `path`, as `-O0` builds it for
/// `lotml run` (specs/llvm-parity R2.2).
pub fn compile_program(
    source: &str,
    path: &Path,
    interfaces: &Interfaces,
    tests: bool,
    lines: bool,
) -> Result<Program, Vec<Diagnostic>> {
    let (lowered, ()) = lowered(source, interfaces, tests, |_, _| Ok(()))?;
    let entry = if tests { Entry::Tests } else { Entry::Main };
    let ll = emit::program(&lowered, &path.display().to_string(), entry, lines)?;
    Ok(Program { ll, libraries: lowered.libraries.iter().cloned().collect() })
}

/// The library for `source`, read from `path`, exporting each function C can call under the
/// name its file's stem gives it (specs/c-abi-export R1.1-R1.5); or the errors that stop it, a
/// module with nothing to export among them.
pub fn compile_library(source: &str, path: &Path, interfaces: &Interfaces) -> Result<Library, Vec<Diagnostic>> {
    let stem = path.file_stem().map_or("module".into(), |s| s.to_string_lossy().into_owned());
    let name = export::module_name(&stem);
    let (lowered, (exports, warnings)) = lowered(source, interfaces, false, |module, checked| {
        let (exports, warnings) = export::exports(&name, module, checked);
        if exports.is_empty() {
            let none = Diagnostic::error(
                "E0404",
                Span::new(0, 0),
                format!(
                    "no function of `{stem}` can be exported to C: a library exports the functions, other than \
                     `main`, whose parameters and result are integers, floats, `bool` or `None`, or `str` among the \
                     parameters"
                ),
            );
            return Err(std::iter::once(none).chain(warnings).collect());
        }
        if let Some(e) = exports.iter().find(|e| e.symbol.starts_with("lt_")) {
            let clash = Diagnostic::error(
                "E0405",
                e.span,
                format!(
                    "`{}` would be exported as `{}`, which the runtime's own functions are named like: name the \
                     file otherwise",
                    e.name, e.symbol
                ),
            );
            return Err(vec![clash]);
        }
        Ok((exports, warnings))
    })?;
    let missing: Vec<Diagnostic> = exports
        .iter()
        .filter(|e| !lowered.functions.iter().any(|f| f.name == lotml_ir::symbol::function(&e.name)))
        .map(|e| Diagnostic::error("E0402", e.span, format!("`{}` was not compiled, so it cannot be exported", e.name)))
        .collect();
    if !missing.is_empty() {
        return Err(missing);
    }
    let file = path.file_name().map_or(stem.clone(), |n| n.to_string_lossy().into_owned());
    let header = export::header(&name, &file, &exports, |span| lowered.line(span));
    let ll = emit::program(&lowered, &path.display().to_string(), Entry::Library(&exports), false)?;
    Ok(Library { ll, header, libraries: lowered.libraries.iter().cloned().collect(), warnings })
}
