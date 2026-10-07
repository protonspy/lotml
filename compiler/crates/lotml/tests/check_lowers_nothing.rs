//! `lotml check` checks a program without lowering it (specs/shared-ir R2.2): what it runs cannot
//! reach the IR or a backend, so the under-100 ms check pays for neither.

use std::path::Path;

const LOWERING: [&str; 4] = ["lotml-ir", "lotml-c", "lotml-py", "lotml-runtime"];

fn read(path: &str) -> String {
    let full = Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("{}: {e}", full.display()))
}

#[test]
fn the_crates_check_runs_on_depend_on_no_lowering() {
    for manifest in [
        "../lotml-check/Cargo.toml",
        "../lotml-db/Cargo.toml",
        "../lotml-syntax/Cargo.toml",
        "../lotml-diag/Cargo.toml",
    ] {
        let text = read(manifest);
        for crate_name in LOWERING {
            assert!(!text.contains(&format!("{crate_name} =")), "{manifest} depends on {crate_name}");
        }
    }
}

#[test]
fn the_check_command_calls_no_lowering() {
    let source = read("src/check.rs");
    for crate_name in LOWERING {
        let path = crate_name.replace('-', "_");
        assert!(!source.contains(&format!("{path}::")), "src/check.rs calls into {crate_name}");
    }
}
