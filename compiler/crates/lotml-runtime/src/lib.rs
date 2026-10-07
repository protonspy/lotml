//! The runtime native programs run on, apart from the compiler that writes them: C, compiled into
//! every program as one translation unit (adr:0016, plans/ir-architecture.md).

use std::path::Path;

/// The runtime's files: `lotml.h` declares it, `lotml.c` includes the rest, and the program
/// includes both, so it is one translation unit.
pub const FILES: &[(&str, &str)] = &[
    ("lotml.h", include_str!("../c/lotml.h")),
    ("lotml.c", include_str!("../c/lotml.c")),
    ("lotml_text.c", include_str!("../c/lotml_text.c")),
    ("lotml_list.c", include_str!("../c/lotml_list.c")),
    ("lotml_dict.c", include_str!("../c/lotml_dict.c")),
];

/// Write the runtime into `dir`, where a compiled program includes it from.
pub fn write(dir: &Path) -> std::io::Result<()> {
    for (name, text) in FILES {
        std::fs::write(dir.join(name), text)?;
    }
    Ok(())
}
