//! Running one program on the LLVM target and on the Python target: the parity the LLVM backend is
//! held to (specs/llvm-backend R2.2).

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use lotml_llvm::driver::{self, Level};

/// What a program did: its standard output and error, and its exit status.
#[derive(Debug)]
pub struct Run {
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
    /// The LLVM IR the backend wrote, for the LLVM target.
    pub ll: String,
}

pub fn scratch(group: &str, name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(group).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

fn finish(out: std::process::Output, ll: String) -> Run {
    Run {
        stdout: String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        stderr: String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
        code: out.status.code(),
        ll,
    }
}

/// The `clang` of this machine, or `None`, saying so, when it has none: the tests then skip.
pub fn clang() -> Option<driver::Clang> {
    match driver::find() {
        Ok(clang) => Some(clang),
        Err(e) => {
            assert!(std::env::var_os("CI").is_none(), "CI has no clang: {e}");
            eprintln!("skipped: {e}");
            None
        }
    }
}

/// `source`, written to `prog.lot`, compiled by the LLVM backend at `level`, built and run.
pub fn run_llvm(clang: &driver::Clang, name: &str, source: &str, level: Level) -> Run {
    let dir = scratch("llvm-target", &format!("{name}-{level:?}"));
    let path = dir.join("prog.lot");
    std::fs::write(&path, source).unwrap();
    let ll = lotml_llvm::compile(source, &path).unwrap_or_else(|d| {
        panic!("{source}\ndoes not compile to LLVM: {:#?}", d.iter().map(|d| &d.message).collect::<Vec<_>>())
    });
    let ll_path = dir.join("prog.ll");
    std::fs::write(&ll_path, &ll).unwrap();
    lotml_runtime::write(&dir).unwrap();
    let exe = dir.join(if cfg!(windows) { "prog.exe" } else { "prog" });
    clang.build(&ll_path, &dir, &exe, level, &[]).unwrap_or_else(|e| panic!("{e}\n--- the IR ---\n{ll}"));
    let out = Command::new(&exe).current_dir(&dir).output().expect("the program runs");
    finish(out, ll)
}

/// `source` compiled by the Python backend and run as `lotml run` runs it.
pub fn run_python(name: &str, source: &str) -> Run {
    let dir = scratch("py-target", name);
    let path = dir.join("prog.lot");
    std::fs::write(&path, source).unwrap();
    let module = lotml_py::compile_with(source, &path, &lotml_check::Interfaces::new())
        .unwrap_or_else(|d| panic!("{source}\n{d:#?}"))
        .module;
    std::fs::write(dir.join("prog.py"), module).unwrap();
    std::fs::write(dir.join("lotml_rt.py"), lotml_py::RUNTIME).unwrap();
    let python = lotml_py::python().expect("a Python interpreter");
    let out = Command::new(&python[0])
        .args(&python[1..])
        .arg("-c")
        .arg("import sys, lotml_rt\nsys.exit(lotml_rt.main('prog'))")
        .current_dir(&dir)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .env("PYTHONHASHSEED", "0")
        .output()
        .expect("Python runs");
    finish(out, String::new())
}

/// `source` on the Python target and on the LLVM target at both levels: the same standard output
/// and exit status, or the test fails. The LLVM target's runs are returned, `-O0` first; `None`
/// when this machine has no `clang`.
pub fn parity(name: &str, source: &str) -> Option<[Run; 2]> {
    let clang = clang()?;
    let python = run_python(name, source);
    let runs = [Level::Debug, Level::Release].map(|level| run_llvm(&clang, name, source, level));
    for (level, run) in [Level::Debug, Level::Release].iter().zip(&runs) {
        assert_eq!(
            (run.stdout.as_str(), run.code),
            (python.stdout.as_str(), python.code),
            "LLVM at {level:?} differs from Python on {name}\n--- LLVM stderr ---\n{}\n--- Python stderr ---\n{}\n--- the IR ---\n{}",
            run.stderr,
            python.stderr,
            run.ll
        );
    }
    Some(runs)
}

/// `source` stops with status 101 on both levels, after writing `before`, with a panic of `kind`
/// whose message holds `message`, naming `prog.lot`, `line` and `function` (R2.4).
pub fn panics(name: &str, source: &str, before: &str, kind_and_message: &str, line: u32, function: &str) {
    let Some(runs) = parity(name, source) else { return };
    for run in &runs {
        assert_eq!(run.code, Some(101), "{name}: {}", run.stderr);
        assert_eq!(run.stdout, before, "{name}");
        assert!(run.stderr.contains(&format!("panic: {kind_and_message}")), "{name}: {}", run.stderr);
        let place = format!("prog.lot\", line {line}, in {function}");
        assert!(run.stderr.contains(&place), "{name}: wants {place}\n{}", run.stderr);
    }
}
