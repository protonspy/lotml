//! The C backend: a checked lotml program as one C file over a counting runtime (R18, R19;
//! adr:0014, specs/c-backend).

pub mod driver;

use std::path::Path;

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
