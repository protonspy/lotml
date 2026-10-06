//! The C backend: a checked lotml program as one C file over a counting runtime (R18, R19;
//! adr:0014, specs/c-backend).

pub mod driver;
mod emit;
mod hoist;
mod lower;
mod mir;
mod own;
mod reuse;
mod types;

use std::path::Path;

use lotml_check::{Interfaces, check_resolved_with};
use lotml_diag::{Diagnostic, Severity};
use lotml_syntax::parse;

/// The runtime's files, written next to every compiled program: `lotml.h` declares it, `lotml.c`
/// includes the rest, and the program includes both, so it is one translation unit.
pub const RUNTIME: &[(&str, &str)] = &[
    ("lotml.h", include_str!("../runtime/lotml.h")),
    ("lotml.c", include_str!("../runtime/lotml.c")),
    ("lotml_text.c", include_str!("../runtime/lotml_text.c")),
    ("lotml_list.c", include_str!("../runtime/lotml_list.c")),
    ("lotml_dict.c", include_str!("../runtime/lotml_dict.c")),
];

/// Write the runtime into `dir`, where a compiled program includes it from.
pub fn write_runtime(dir: &Path) -> std::io::Result<()> {
    for (name, text) in RUNTIME {
        std::fs::write(dir.join(name), text)?;
    }
    Ok(())
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
    for f in &mut lowered.functions {
        own::insert_counts(f);
        reuse::insert_reuse(f);
        hoist::hoist_uniqueness(f);
    }
    let c = emit::program(&lowered, &path.display().to_string(), tests);
    Ok(Program { c, libraries: lowered.libraries.iter().filter(|l| !driver::linked_always(l)).cloned().collect() })
}
