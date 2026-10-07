//! `clang` builds LLVM IR with the runtime into a program (specs/llvm-backend R1.1, R1.5).

use std::path::{Path, PathBuf};
use std::process::Command;

use lotml_llvm::cache::Cache;
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

/// The hello program built into `dir` at `level` through `cache`, run; whether the runtime was
/// compiled for it.
fn build_through(clang: &driver::Clang, dir: &Path, build: driver::Build, cache: &Cache) -> driver::Runtime {
    lotml_runtime::write(dir).unwrap();
    let ll = dir.join("prog.ll");
    std::fs::write(&ll, HELLO).unwrap();
    let exe = dir.join(if cfg!(windows) { "prog.exe" } else { "prog" });
    let runtime = clang.link_with(&ll, dir, &exe, &[], build, Some(cache)).unwrap_or_else(|e| panic!("{e}"));
    let out = Command::new(&exe).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hi\n");
    runtime
}

#[test]
fn a_second_build_with_the_same_flags_takes_the_runtime_from_the_cache() {
    let Some(clang) = clang() else { return };
    let cache = Cache::at(&scratch("cache-reuse")).expect("a cache");
    let debug = driver::Build { level: Level::Debug, counting: false, shared: false };
    assert_eq!(build_through(&clang, &scratch("reuse-first"), debug, &cache), driver::Runtime::Compiled);
    assert_eq!(build_through(&clang, &scratch("reuse-second"), debug, &cache), driver::Runtime::Cached);
    let release = driver::Build { level: Level::Release, ..debug };
    assert_eq!(build_through(&clang, &scratch("reuse-release"), release, &cache), driver::Runtime::Compiled);
    let counting = driver::Build { counting: true, ..debug };
    assert_eq!(build_through(&clang, &scratch("reuse-counting"), counting, &cache), driver::Runtime::Compiled);
}

#[test]
fn a_damaged_entry_compiles_the_runtime_again() {
    let Some(clang) = clang() else { return };
    let cache = Cache::at(&scratch("cache-damaged")).expect("a cache");
    let debug = driver::Build { level: Level::Debug, counting: false, shared: false };
    assert_eq!(build_through(&clang, &scratch("damaged-first"), debug, &cache), driver::Runtime::Compiled);
    for entry in std::fs::read_dir(cache.dir()).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().is_some_and(|e| e == "o") {
            std::fs::write(path, "damaged").unwrap();
        }
    }
    assert_eq!(build_through(&clang, &scratch("damaged-again"), debug, &cache), driver::Runtime::Compiled);
    assert_eq!(build_through(&clang, &scratch("damaged-third"), debug, &cache), driver::Runtime::Cached);
}

#[test]
fn a_cached_runtime_is_the_compiler_s_own_whatever_the_build_directory_holds() {
    let Some(clang) = clang() else { return };
    let cache = Cache::at(&scratch("cache-own")).expect("a cache");
    let debug = driver::Build { level: Level::Debug, counting: false, shared: false };
    assert_eq!(build_through(&clang, &scratch("own-first"), debug, &cache), driver::Runtime::Compiled);
    let planted = scratch("own-planted");
    lotml_runtime::write(&planted).unwrap();
    std::fs::write(planted.join("stdio.h"), "#error a header planted in the build directory\n").unwrap();
    std::fs::write(planted.join("lotml.c"), "#error a runtime planted in the build directory\n").unwrap();
    let ll = planted.join("prog.ll");
    std::fs::write(&ll, HELLO).unwrap();
    let exe = planted.join(if cfg!(windows) { "prog.exe" } else { "prog" });
    let runtime = clang.link_with(&ll, &planted, &exe, &[], debug, Some(&cache)).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(runtime, driver::Runtime::Cached, "the build directory's files neither key nor compile it");
    let out = Command::new(&exe).output().unwrap();
    assert_eq!(String::from_utf8_lossy(&out.stdout), "hi\n");
}
