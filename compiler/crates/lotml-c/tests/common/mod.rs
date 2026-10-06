//! Running one program on both targets: the parity the C backend is held to (R1.2).

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// What a program did: its standard output and error, and its exit status.
#[derive(Debug)]
pub struct Run {
    pub stdout: String,
    pub stderr: String,
    pub code: Option<i32>,
    /// The C the backend wrote, for the C target.
    pub c: String,
}

pub fn scratch(group: &str, name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(group).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

fn finish(out: std::process::Output, c: String) -> Run {
    Run {
        stdout: String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n"),
        stderr: String::from_utf8_lossy(&out.stderr).replace("\r\n", "\n"),
        code: out.status.code(),
        c,
    }
}

/// `source` compiled by the C backend, built and run; `counting` builds it reporting the cells
/// left at exit.
pub fn run_c_with(name: &str, source: &str, counting: bool) -> Run {
    let dir = scratch("c-target", name);
    let path = dir.join("prog.lotml");
    std::fs::write(&path, source).unwrap();
    let c = lotml_c::compile(source, &path).unwrap_or_else(|d| {
        panic!("{source}\ndoes not compile to C: {:#?}", d.iter().map(|d| &d.message).collect::<Vec<_>>())
    });
    let c = if counting { format!("#define LT_COUNT_CELLS 1\n{c}") } else { c };
    std::fs::write(dir.join("prog.c"), &c).unwrap();
    lotml_c::write_runtime(&dir).unwrap();
    let compiler = lotml_c::driver::find().expect("a C compiler");
    let exe = dir.join(if cfg!(windows) { "prog.exe" } else { "prog" });
    let sanitize = std::env::var_os("LOTML_SANITIZE").is_some();
    compiler.build_with(&dir.join("prog.c"), &exe, &[], sanitize).unwrap_or_else(|e| panic!("{e}\n--- the C ---\n{c}"));
    let out =
        Command::new(&exe).current_dir(&dir).env("ASAN_OPTIONS", "detect_leaks=0").output().expect("the program runs");
    let run = finish(out, c);
    assert!(
        !run.stderr.contains("Sanitizer") && !run.stderr.contains("runtime error:"),
        "{name}: the sanitizer stopped the program\n{}",
        run.stderr
    );
    run
}

pub fn run_c(name: &str, source: &str) -> Run {
    run_c_with(name, source, false)
}

/// `source` compiled by the Python backend and run as `lotml run` runs it.
pub fn run_python(name: &str, source: &str) -> Run {
    let dir = scratch("py-target", name);
    let path = dir.join("prog.lotml");
    std::fs::write(&path, source).unwrap();
    let module = lotml_py::compile(source, &path).unwrap_or_else(|d| panic!("{source}\n{d:#?}"));
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

/// `source` on both targets: the same standard output and exit status, or the test fails. The
/// C target's run is returned.
pub fn parity(name: &str, source: &str) -> Run {
    let python = run_python(name, source);
    let c = run_c(name, source);
    assert_eq!(
        (c.stdout.as_str(), c.code),
        (python.stdout.as_str(), python.code),
        "C differs from Python on {name}\n--- C stderr ---\n{}\n--- Python stderr ---\n{}",
        c.stderr,
        python.stderr
    );
    c
}
