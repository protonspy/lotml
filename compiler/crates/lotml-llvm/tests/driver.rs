//! `clang` builds LLVM IR with the runtime into a program (specs/llvm-backend R1.1, R1.5).

use std::path::{Path, PathBuf};
use std::process::Command;

use lotml_llvm::driver::{self, Level};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("llvm-driver").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

/// The `clang` of this machine, or `None`, saying so, when it has none: the tests then skip.
fn clang() -> Option<driver::Clang> {
    match driver::find() {
        Ok(clang) => Some(clang),
        Err(e) => {
            assert!(std::env::var_os("CI").is_none(), "CI has no clang: {e}");
            eprintln!("skipped: {e}");
            None
        }
    }
}

const HELLO: &str = r#"@text = private unnamed_addr constant [3 x i8] c"hi\0A"

declare void @lt_init()
declare void @lt_write(ptr, i64)
declare i32 @lt_exit(i32)

define i32 @main() {
entry:
  call void @lt_init()
  call void @lt_write(ptr @text, i64 3)
  %status = call i32 @lt_exit(i32 0)
  ret i32 %status
}
"#;

#[test]
fn llvm_ir_and_the_runtime_build_into_a_program_at_both_levels() {
    let Some(clang) = clang() else { return };
    for level in [Level::Debug, Level::Release] {
        let dir = scratch(&format!("hello-{level:?}"));
        lotml_runtime::write(&dir).unwrap();
        let ll = dir.join("prog.ll");
        std::fs::write(&ll, HELLO).unwrap();
        let exe = dir.join(if cfg!(windows) { "prog.exe" } else { "prog" });
        clang.build(&ll, &dir, &exe, level, &[]).unwrap_or_else(|e| panic!("{e}"));
        let out = Command::new(&exe).output().unwrap();
        assert_eq!(String::from_utf8_lossy(&out.stdout), "hi\n");
        assert_eq!(out.status.code(), Some(0));
    }
}

#[test]
fn ir_clang_rejects_is_reported_as_a_compiler_bug_and_kept() {
    let Some(clang) = clang() else { return };
    let dir = scratch("broken");
    lotml_runtime::write(&dir).unwrap();
    let ll = dir.join("prog.ll");
    std::fs::write(&ll, "define i32 @main() {\nentry:\n  ret i32 %nothing\n}\n").unwrap();
    let exe = dir.join("prog");
    let error = clang.build(&ll, &dir, &exe, Level::Debug, &[]).expect_err("the IR is not valid");
    assert!(error.starts_with("a bug in the LotML compiler"), "{error}");
    assert!(error.contains("prog.ll"), "{error}");
    assert!(ll.is_file(), "the IR is kept to report");
}
