//! Line tables (specs/llvm-parity R2.2): a program compiled for `lotml run` carries, at `-O0`, a
//! line table naming its `.lot` file and the lines of its statements, read back from the object
//! by `llvm-dwarfdump --debug-line`, or by `llvm-objdump --line-numbers` where LLVM's Windows
//! installer ships no `llvm-dwarfdump`; one built for `lotml build` carries none.

mod common;

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use common::{clang, scratch};

const PROGRAM: &str = "fn twice(n: int) -> int:\n    m = n * 2\n    return m\n\n\
fn main():\n    var total = 0\n    for i in range(3):\n        total += twice(i)\n    print(total)\n";

/// A tool of `clang`'s LLVM that reads line tables: beside `clang`, on `PATH` by its plain or its
/// versioned name, or under Debian's `/usr/lib/llvm-<major>/bin`.
fn line_reader(clang: &lotml_llvm::driver::Clang) -> Option<PathBuf> {
    let exe = if cfg!(windows) { ".exe" } else { "" };
    let mut dirs: Vec<PathBuf> = clang.program.parent().map(Path::to_path_buf).into_iter().collect();
    dirs.extend(std::env::var_os("PATH").map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).unwrap_or_default());
    dirs.push(PathBuf::from(format!("/usr/lib/llvm-{}/bin", clang.version)));
    for tool in ["llvm-dwarfdump", "llvm-objdump"] {
        for name in [tool.to_string(), format!("{tool}-{}", clang.version)] {
            if let Some(found) = dirs.iter().map(|d| d.join(format!("{name}{exe}"))).find(|p| p.is_file()) {
                return Some(found);
            }
        }
    }
    None
}

/// The lines of `prog.lot` the object's line table names.
fn lines_named(reader: &Path, object: &Path) -> BTreeSet<u32> {
    let dwarfdump = reader.file_stem().is_some_and(|s| s.to_string_lossy().starts_with("llvm-dwarfdump"));
    let args: &[&str] = if dwarfdump { &["--debug-line"] } else { &["--line-numbers"] };
    let out = Command::new(reader).args(args).arg(object).output().expect("the line reader runs");
    let text = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(text.contains("prog.lot"), "the line table names the .lot file:\n{text}");
    if dwarfdump {
        // A row of the table: `0x<address> <line> <column> <file> …`, after the file table.
        text.lines()
            .filter(|l| l.starts_with("0x"))
            .filter_map(|l| l.split_whitespace().nth(1)?.parse().ok())
            .filter(|&line| line > 0)
            .collect()
    } else {
        text.lines().filter_map(|l| l.strip_prefix("; ")?.rsplit_once("prog.lot:")?.1.trim().parse().ok()).collect()
    }
}

#[test]
fn a_debug_build_names_the_lot_lines_of_its_statements() {
    let Some(clang) = clang() else { return };
    let Some(reader) = line_reader(&clang) else {
        assert!(std::env::var_os("CI").is_none(), "CI has no llvm-dwarfdump or llvm-objdump beside clang");
        eprintln!("skipped: no llvm-dwarfdump or llvm-objdump beside {}", clang.program.display());
        return;
    };
    let dir = scratch("lines", "debug");
    let path = dir.join("prog.lot");
    std::fs::write(&path, PROGRAM).unwrap();
    let program = lotml_llvm::compile_program(PROGRAM, &path, &lotml_check::Interfaces::new(), false, true)
        .unwrap_or_else(|d| panic!("{d:#?}"));
    let ll = dir.join("prog.ll");
    std::fs::write(&ll, &program.ll).unwrap();
    lotml_runtime::write(&dir).unwrap();
    let object = dir.join("prog.o");
    let built =
        Command::new(&clang.program).args(["-c", "-O0", "-g", "-w", "-o"]).arg(&object).arg(&ll).output().unwrap();
    assert!(built.status.success(), "{}", String::from_utf8_lossy(&built.stderr));
    let lines = lines_named(&reader, &object);
    for line in [2, 3, 6, 7, 8, 9] {
        assert!(lines.contains(&line), "line {line} of prog.lot is in the table: {lines:?}");
    }
    assert!(lines.iter().all(|&l| l <= 9), "every line is one of prog.lot's: {lines:?}");
}

#[test]
fn a_release_build_carries_no_line_table() {
    let program =
        lotml_llvm::compile_program(PROGRAM, Path::new("prog.lot"), &lotml_check::Interfaces::new(), false, false)
            .unwrap_or_else(|d| panic!("{d:#?}"));
    assert!(!program.ll.contains("!dbg") && !program.ll.contains("DICompileUnit"), "{}", program.ll);
}
