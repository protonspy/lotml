//! C libraries and Python modules on the C target: a function of a `c.<library>` interface is
//! called directly and the library linked; a Python module cannot be imported (R5.3, R5.4).

mod common;

use std::path::Path;
use std::process::Command;

use common::{run_python_with, scratch};
use lotml_check::{Interfaces, interface_of};

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

/// `source` built for the C target with `interfaces`, or the build's error.
fn build(name: &str, source: &str, interfaces: &Interfaces) -> Result<std::path::PathBuf, String> {
    let dir = scratch("c-libraries", name);
    let path = dir.join("prog.lotml");
    std::fs::write(&path, source).unwrap();
    let program = lotml_c::compile_program(source, &path, interfaces, false)
        .unwrap_or_else(|d| panic!("{source}\n{:#?}", d.iter().map(|d| &d.message).collect::<Vec<_>>()));
    std::fs::write(dir.join("prog.c"), &program.c).unwrap();
    lotml_c::write_runtime(&dir).unwrap();
    let exe = dir.join(if cfg!(windows) { "prog.exe" } else { "prog" });
    let compiler = lotml_c::driver::find().expect("a C compiler");
    let sanitize = std::env::var_os("LOTML_SANITIZE").is_some();
    compiler.build_with(&dir.join("prog.c"), &exe, &program.libraries, sanitize)?;
    Ok(exe)
}

fn stdout_of(exe: &Path) -> String {
    let out = Command::new(exe).env("ASAN_OPTIONS", "detect_leaks=0").output().expect("the program runs");
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n")
}

#[test]
fn a_c_function_is_called_directly_as_on_the_python_target() {
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
    let exe = build("values", &source, &read).unwrap_or_else(|e| panic!("{e}"));
    let c = stdout_of(&exe);
    assert_eq!(c, "42 6 65\n1.0 1024.0\n");
    let python = run_python_with("c-libraries-values", &source, &read);
    assert_eq!(c, python.stdout, "{}", python.stderr);
}

#[test]
fn a_python_import_is_refused_at_the_import() {
    let source = "from textwrap import fill\n\nfn main():\n    print(1)\n";
    let read = interfaces(&[("textwrap", "fn fill(text: str, width: int) -> str ! PyError\n")]);
    let path = Path::new("prog.lotml");
    let Err(errors) = lotml_c::compile_program(source, path, &read, false) else {
        panic!("a Python import compiled to C");
    };
    let refused: Vec<_> = errors.iter().filter(|d| d.code == "E0401").collect();
    assert_eq!(refused.len(), 1, "{:#?}", errors.iter().map(|d| (&d.code, &d.message)).collect::<Vec<_>>());
    assert_eq!(refused[0].span.start, 0, "the import is what is refused");
}

#[test]
fn a_missing_library_stops_the_build() {
    let source = "from c.no_such_library_here import f\n\nfn main():\n    print(f(1))\n";
    let read = interfaces(&[("c.no_such_library_here", "fn f(x: i32) -> i32\n")]);
    let error = build("missing", source, &read).expect_err("the library is not there to link");
    assert!(error.contains("no_such_library_here"), "{error}");
}
