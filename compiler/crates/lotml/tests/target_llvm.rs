//! `--target llvm` on `lotml run` and `build`: the program compiled to LLVM IR and built by
//! `clang`, `-O0` for `run` and `-O2` for `build` (specs/llvm-backend R1.1, R1.3, R1.4).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn lotml(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lotml")).current_dir(dir).args(args).output().expect("the binary runs")
}

fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("target-llvm").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    for (file, text) in files {
        std::fs::write(dir.join(file), text).expect("a scratch file");
    }
    dir
}

/// Whether this machine has a `clang` the backend takes; the tests that build skip, saying so,
/// when it has none.
fn has_clang() -> bool {
    match lotml_llvm::driver::find() {
        Ok(_) => true,
        Err(e) => {
            assert!(std::env::var_os("CI").is_none(), "CI has no clang: {e}");
            eprintln!("skipped: {e}");
            false
        }
    }
}

const ADD: &str = "fn add(a: f64, b: f64) -> f64:\n    return a + b\n\nfn main():\n    print(add(1.5, 2.25))\n";

#[test]
fn run_on_the_llvm_target_prints_and_exits_as_python() {
    if !has_clang() {
        return;
    }
    let dir = scratch("run", &[("add.lot", ADD), ("stop.lot", "fn main():\n    print(1)\n    print(1 // 0)\n")]);
    let text = |out: &Output| String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    let llvm = lotml(&["run", "--target", "llvm", "add.lot"], &dir);
    assert_eq!(llvm.status.code(), Some(0), "{}", String::from_utf8_lossy(&llvm.stderr));
    assert_eq!(text(&llvm), "3.75\n");
    assert_eq!(text(&llvm), text(&lotml(&["run", "add.lot"], &dir)));
    let stopped = lotml(&["run", "--target", "llvm", "stop.lot"], &dir);
    assert_eq!(stopped.status.code(), Some(101));
    assert_eq!(text(&stopped), "1\n");
    assert!(String::from_utf8_lossy(&stopped.stderr).contains("line 3, in main"));
}

#[test]
fn build_on_the_llvm_target_writes_an_executable_and_its_ir() {
    if !has_clang() {
        return;
    }
    let dir = scratch("build", &[("add.lot", ADD)]);
    let out = lotml(&["build", "--target", "llvm", "add.lot", "-o", "out"], &dir);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(dir.join("out").join("add.ll").is_file());
    let exe = dir.join("out").join(if cfg!(windows) { "add.exe" } else { "add" });
    let ran = Command::new(&exe).output().expect("the executable runs");
    assert_eq!(String::from_utf8_lossy(&ran.stdout).replace("\r\n", "\n"), "3.75\n");
}

#[test]
fn without_clang_the_llvm_target_says_where_it_looked_and_what_else_builds() {
    let dir = scratch("no-clang", &[("add.lot", ADD)]);
    let out = Command::new(env!("CARGO_BIN_EXE_lotml"))
        .current_dir(&dir)
        .env("LOTML_CLANG", "no-such-clang-anywhere")
        .args(["build", "--target", "llvm", "add.lot", "-o", "out"])
        .output()
        .expect("the binary runs");
    assert_eq!(out.status.code(), Some(2));
    let said = String::from_utf8_lossy(&out.stderr);
    assert!(said.contains("LOTML_CLANG"), "{said}");
    assert!(said.contains("clang 17 or newer"), "{said}");
    assert!(said.contains("--target python"), "{said}");
}

#[test]
fn test_on_the_llvm_target_reports_what_the_python_target_reports() {
    if !has_clang() {
        return;
    }
    let module = "type P(x: int, name: str)\n\nfn grow(xs: [int]) -> [int]:\n    return xs + [len(xs)]\n\n\
                  test \"passes\":\n    assert grow([1]) == [1, 1]\n\n\
                  test \"fails\":\n    assert P(1, \"a\") == P(2, \"b\")\n\n\
                  test \"panics\":\n    xs = [1]\n    print(xs[3])\n";
    let dir = scratch("test", &[("t.lot", module)]);
    let report = |target: &str| {
        let out = lotml(&["test", "--target", target, "--json", "t.lot"], &dir);
        assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n")
    };
    let llvm = report("llvm");
    assert!(llvm.contains("\"outcome\":\"fail\"") || llvm.contains("\"outcome\": \"fail\""), "{llvm}");
    assert_eq!(llvm, report("python"));
}
