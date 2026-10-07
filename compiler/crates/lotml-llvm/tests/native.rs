//! What a native program does beyond the language's values: `parallel` on the runtime's threads,
//! and a `c.<library>` function called directly with its library linked (specs/llvm-parity R3.2,
//! R4.1).

mod common;

use std::path::Path;
use std::process::Command;

use common::{clang, frees_everything, parity, run_python_with, scratch};
use lotml_check::{Interfaces, interface_of};
use lotml_llvm::driver::Level;

const PARALLEL: &str = include_str!("programs/parallel.lotml");

#[test]
fn parallel_runs_the_tasks_and_keeps_their_order() {
    let Some([run, _]) = parity("parallel", PARALLEL) else { return };
    assert!(run.stdout.starts_with("[300000, 15, 3]\n300 task 0 ab task 1 cd task 299 cd\n"), "{}", run.stdout);
    assert!(run.ll.contains("@lt_parallel("), "the tasks run through the runtime's threads");
    frees_everything("parallel-free", PARALLEL);
}

#[test]
fn a_task_that_panics_stops_the_program_after_the_others() {
    let source = "fn boom(n: int) -> int:\n    return 10 // n\n\nfn main():\n    print(\"before\")\n    \
                  r = parallel([lambda: boom(1), lambda: boom(0), lambda: boom(2)])\n    print(r)\n";
    let Some(runs) = parity("parallel-panic", source) else { return };
    for run in &runs {
        assert_eq!(run.code, Some(101));
        assert_eq!(run.stdout, "before\n");
        assert!(run.stderr.contains("ZeroDivisionError"), "{}", run.stderr);
    }
}

/// The C runtime library, as the Python target's tests name it, and the one holding `cos`.
fn libc() -> &'static str {
    if cfg!(windows) { "c.msvcrt" } else { "c.c" }
}

fn libm() -> &'static str {
    if cfg!(windows) { "c.msvcrt" } else { "c.m" }
}

fn interfaces(list: &[(&str, &str)]) -> Interfaces {
    let mut read = Interfaces::new();
    for (module, text) in list {
        let (found, problems) = interface_of(module, text);
        assert!(problems.is_empty(), "{module}: {:?}", problems.iter().map(|d| &d.message).collect::<Vec<_>>());
        read.insert((*module).to_string(), found);
    }
    read
}

/// `source` built for the LLVM target with `interfaces`, or the build's error.
fn build(
    clang: &lotml_llvm::driver::Clang,
    name: &str,
    source: &str,
    interfaces: &Interfaces,
) -> Result<std::path::PathBuf, String> {
    let dir = scratch("c-libraries", name);
    let path = dir.join("prog.lot");
    std::fs::write(&path, source).unwrap();
    let program = lotml_llvm::compile_program(source, &path, interfaces, false, false)
        .unwrap_or_else(|d| panic!("{source}\n{:#?}", d.iter().map(|d| &d.message).collect::<Vec<_>>()));
    let ll = dir.join("prog.ll");
    std::fs::write(&ll, &program.ll).unwrap();
    lotml_runtime::write(&dir).unwrap();
    let exe = dir.join(if cfg!(windows) { "prog.exe" } else { "prog" });
    clang.build(&ll, &dir, &exe, Level::Release, &program.libraries)?;
    Ok(exe)
}

fn stdout_of(exe: &Path) -> String {
    let out = Command::new(exe).output().expect("the program runs");
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n")
}

#[test]
fn a_c_function_is_called_directly_as_on_the_python_target() {
    let Some(clang) = clang() else { return };
    let (library, math) = (libc(), libm());
    let source = format!(
        "from {library} import labs, strlen, toupper\nfrom {math} import cos, pow\n\n\
         fn main():\n    print(labs(-42), strlen(\"héllo\"), toupper(97))\n    print(cos(0.0), pow(2.0, 10.0))\n"
    );
    let text = "fn labs(n: i64) -> i64\nfn strlen(s: str) -> u64\nfn toupper(c: i32) -> i32\n";
    let maths = "fn cos(x: f64) -> f64\nfn pow(x: f64, y: f64) -> f64\n";
    let both = format!("{text}{maths}");
    let read = if library == math {
        interfaces(&[(library, both.as_str())])
    } else {
        interfaces(&[(library, text), (math, maths)])
    };
    let exe = build(&clang, "values", &source, &read).unwrap_or_else(|e| panic!("{e}"));
    let native = stdout_of(&exe);
    assert_eq!(native, "42 6 65\n1.0 1024.0\n");
    let python = run_python_with("c-libraries-values", &source, &read);
    assert_eq!(native, python.stdout, "{}", python.stderr);
}

#[test]
fn a_c_function_taking_f32_or_returning_a_byte_or_bool_is_called_as_c_declares_it() {
    let Some(clang) = clang() else { return };
    let (library, math) = (libc(), libm());
    let source = format!(
        "from {library} import toupper, isdigit\nfrom {math} import sqrtf\n\n\
         fn main():\n    print(sqrtf(f32(2.25)), toupper(98))\n    if isdigit(55) or True:\n        print(\"called\")\n"
    );
    let text = "fn toupper(c: i32) -> u8\nfn isdigit(c: i32) -> bool\n";
    let maths = "fn sqrtf(x: f32) -> f32\n";
    let both = format!("{text}{maths}");
    let read = if library == math {
        interfaces(&[(library, both.as_str())])
    } else {
        interfaces(&[(library, text), (math, maths)])
    };
    let exe = build(&clang, "narrow", &source, &read).unwrap_or_else(|e| panic!("{e}"));
    let native = stdout_of(&exe);
    assert_eq!(native, "1.5 66\ncalled\n");
    let python = run_python_with("c-libraries-narrow", &source, &read);
    assert_eq!(native, python.stdout, "{}", python.stderr);
}

#[test]
fn a_missing_library_stops_the_build() {
    let Some(clang) = clang() else { return };
    let source = "from c.no_such_library_here import f\n\nfn main():\n    print(f(1))\n";
    let read = interfaces(&[("c.no_such_library_here", "fn f(x: i32) -> i32\n")]);
    let error = build(&clang, "missing", source, &read).expect_err("the library is not there to link");
    assert!(error.contains("no_such_library_here"), "{error}");
}
