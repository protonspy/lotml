//! The platform's C compiler builds a program with the runtime (R5.1).

use std::path::{Path, PathBuf};
use std::process::Command;

use lotml_c::driver;

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("c-driver").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

const PROGRAM: &str = r#"#include "lotml.h"
#include "lotml.c"

int main(int argc, char **argv) {
    lt_init();
    lt_write("one\ntwo\n", 8);
    if (argc > 1) {
        lt_panic(&(lt_at){"prog.lotml", 3, "main"}, "Overflow", "9223372036854775808 does not fit in int");
    }
    return lt_exit(0);
}
"#;

fn build(name: &str) -> PathBuf {
    let dir = scratch(name);
    lotml_runtime::write(&dir).unwrap();
    std::fs::write(dir.join("prog.c"), PROGRAM).unwrap();
    let compiler = driver::find().expect("a C compiler on this machine");
    let exe = dir.join(if cfg!(windows) { "prog.exe" } else { "prog" });
    compiler.build(&dir.join("prog.c"), &exe, &[]).unwrap_or_else(|e| panic!("{e}"));
    exe
}

#[test]
fn a_program_built_with_the_runtime_writes_its_output_as_bytes() {
    let exe = build("output");
    let out = Command::new(&exe).output().unwrap();
    assert_eq!(out.stdout, b"one\ntwo\n", "\\n is never written as \\r\\n");
    assert!(out.status.success());
}

#[test]
fn a_panic_flushes_the_output_and_names_the_lotml_line() {
    let exe = build("panic");
    let out = Command::new(&exe).arg("panic").output().unwrap();
    assert_eq!(out.status.code(), Some(101));
    assert_eq!(out.stdout, b"one\ntwo\n");
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.starts_with("panic: Overflow: 9223372036854775808 does not fit in int\n"), "{stderr}");
    assert!(stderr.contains("File \"prog.lotml\", line 3, in main"), "{stderr}");
}

#[test]
fn a_build_that_fails_returns_the_compiler_s_output() {
    let dir = scratch("broken");
    std::fs::write(dir.join("bad.c"), "int main(void) { return undeclared_name; }\n").unwrap();
    let compiler = driver::find().expect("a C compiler on this machine");
    let error = compiler.build(&dir.join("bad.c"), &dir.join("bad.exe"), &[]).unwrap_err();
    assert!(error.contains("undeclared_name"), "{error}");
}
