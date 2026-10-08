//! C libraries on the Python target (R17, adr:0013): a C function is loaded when the module
//! loads and called with the interpreter released, so a call that blocks holds up only its task.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use lotml_check::{Interfaces, interface_of};
use lotml_py::{RUNTIME, compile_with, python};

/// The C runtime library, and a function that blocks for a number of milliseconds there.
fn platform() -> (&'static str, &'static str, &'static str) {
    if cfg!(windows) {
        ("c.msvcrt", "c.kernel32", "fn Sleep(ms: u32)\n")
    } else {
        ("c.c", "c.c", "fn usleep(us: u32) -> i32\n")
    }
}

fn run(name: &str, source: &str, interfaces: &[(&str, &str)]) -> Output {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("c-libraries").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let path: PathBuf = dir.join("prog.lotml");
    std::fs::write(&path, source).unwrap();
    let mut read = Interfaces::new();
    for (module, text) in interfaces {
        let (found, problems) = interface_of(module, text);
        assert!(problems.is_empty(), "{module}: {:?}", problems.iter().map(|d| &d.message).collect::<Vec<_>>());
        read.insert((*module).to_string(), found);
    }
    let compiled = compile_with(source, &path, &read).unwrap_or_else(|d| panic!("{source}\n{d:#?}"));
    std::fs::write(dir.join("prog.py"), compiled.module).unwrap();
    std::fs::write(dir.join("lotml_rt.py"), RUNTIME).unwrap();
    let python = python().expect("a Python interpreter");
    Command::new(&python[0])
        .args(&python[1..])
        .arg("-c")
        .arg("import sys, lotml_rt\nsys.exit(lotml_rt.main('prog'))")
        .current_dir(&dir)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .output()
        .expect("Python runs")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n")
}

#[test]
fn a_c_function_takes_numbers_and_strings_and_returns_its_value() {
    let (libc, _, _) = platform();
    let source =
        format!("from {libc} import labs, strlen\n\nfn main():\n    print(labs(-42))\n    print(strlen(\"héllo\"))\n");
    let out = run("values", &source, &[(libc, "fn labs(n: i64) -> i64\nfn strlen(s: str) -> u64\n")]);
    assert_eq!(stdout(&out), "42\n6\n", "a str crosses as UTF-8: {}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn a_c_call_that_blocks_holds_up_only_its_task() {
    let (_, library, declaration) = platform();
    let (function, wait) = if cfg!(windows) { ("Sleep", "400") } else { ("usleep", "400000") };
    let source = format!(
        "from {library} import {function}\nfrom py.time import monotonic\n\n\
         fn nap() -> int:\n    {function}({wait})\n    return 1\n\n\
         fn main() -> None ! PyError:\n    start = monotonic()?\n\
         \x20   done = sum(parallel([lambda: nap(), lambda: nap(), lambda: nap(), lambda: nap()]))\n\
         \x20   print(done)\n    print(monotonic()? - start < 1.2)\n"
    );
    let out = run("blocking", &source, &[(library, declaration), ("py.time", "fn monotonic() -> f64 ! PyError\n")]);
    assert_eq!(stdout(&out), "4\nTrue\n", "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn a_missing_library_or_function_stops_the_program_when_it_loads() {
    let out = run(
        "missing",
        "from c.no_such_library_here import f\n\nfn main():\n    print(f(1))\n",
        &[("c.no_such_library_here", "fn f(x: i32) -> i32\n")],
    );
    assert_eq!(out.status.code(), Some(101));
    assert!(String::from_utf8_lossy(&out.stderr).contains("LinkError"), "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn a_string_with_a_nul_byte_is_never_cut_short_in_c() {
    let (libc, _, _) = platform();
    // The lotml source holds the escape `\x00`, a NUL in the string's value.
    let source = format!("from {libc} import strlen\n\nfn main():\n    print(strlen(\"a\\x00b\"))\n");
    let out = run("nul-byte", &source, &[(libc, "fn strlen(s: str) -> u64\n")]);
    assert_eq!(out.status.code(), Some(101), "{}", stdout(&out));
    assert!(String::from_utf8_lossy(&out.stderr).contains("NUL byte"), "{}", String::from_utf8_lossy(&out.stderr));
}
