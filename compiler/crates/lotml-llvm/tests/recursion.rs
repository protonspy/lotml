//! Recursion on both targets (specs/recursion-depth R1.1–R1.6, R2.1–R2.3): programs that recurse
//! to the limit of 1,000 calls and one past it, directly, mutually, through a lambda, through
//! `dyn`, through `parallel`, with large frames and in `test` blocks, print and stop alike on the
//! Python target, on every interpreter `LOTML_PYTHONS` names, and on the LLVM target at `-O0` and
//! at `-O2`; and a library's exported function recursing from its C host stops as a program does.

mod common;

use std::path::Path;
use std::process::Command;

use common::{Build, Run, build_and_run, clang, pythons, run_llvm, run_python_on, scratch};
use lotml_check::Interfaces;
use lotml_llvm::driver::Level;
use lotml_llvm::export::library_file;
use serde_json::Value;

const LEVELS: [Level; 2] = [Level::Debug, Level::Release];

/// The first line of what stopped a run: `panic: <kind>: <message>`, or nothing.
fn stopped_by(run: &Run) -> &str {
    run.stderr.lines().next().filter(|l| l.starts_with("panic: ")).unwrap_or("")
}

const PAST: &str = "panic: RecursionError: maximum recursion depth exceeded";

/// `source` on every interpreter and on the LLVM target at both levels: the same output, status
/// and panic; that output and panic returned, or `None` without `clang`.
fn same(name: &str, source: &str) -> Option<(String, String)> {
    let clang = clang()?;
    let mut seen: Option<(String, Option<i32>, String)> = None;
    for (k, python) in pythons().iter().enumerate() {
        let run = run_python_on(python, &format!("{name}-{k}"), source, &Interfaces::new());
        let found = (run.stdout.clone(), run.code, stopped_by(&run).to_string());
        if let Some(seen) = &seen {
            assert_eq!(&found, seen, "{name}: {python:?} differs from the first interpreter: {}", run.stderr);
        }
        seen = Some(found);
    }
    let (stdout, code, panic) = seen.expect("an interpreter");
    for level in LEVELS {
        let run = run_llvm(&clang, name, source, level);
        assert_eq!(
            (run.stdout.as_str(), run.code, stopped_by(&run)),
            (stdout.as_str(), code, panic.as_str()),
            "{name}: LLVM at {level:?} differs from Python: {}",
            run.stderr
        );
    }
    Some((stdout, panic))
}

const DOWN: &str = "fn down(n: int) -> int:\n    if n == 0:\n        return 0\n    return down(n - 1) + 1\n\n";

#[test]
fn a_function_calling_itself_runs_a_thousand_deep_and_stops_one_past() {
    let source = format!("{DOWN}fn main():\n    print(down(999))\n    print(down(1000))\n");
    if let Some((stdout, panic)) = same("direct", &source) {
        assert_eq!((stdout.as_str(), panic.as_str()), ("999\n", PAST));
    }
}

#[test]
fn functions_calling_each_other_share_the_limit() {
    let source = "fn even(n: int) -> bool:\n    if n == 0:\n        return True\n    return odd(n - 1)\n\n\
fn odd(n: int) -> bool:\n    if n == 0:\n        return False\n    return even(n - 1)\n\n\
fn main():\n    print(even(999))\n    print(even(1000))\n";
    if let Some((stdout, panic)) = same("mutual", source) {
        assert_eq!((stdout.as_str(), panic.as_str()), ("False\n", PAST));
    }
}

#[test]
fn a_recursion_through_a_lambda_counts_the_lambda_too() {
    // Each level is a call of `go` and a call of the lambda: 500 levels are 1,000 calls.
    let source = "fn go(n: int) -> int:\n    if n == 0:\n        return 0\n    again = [lambda m: go(m)]\n    return again[0](n - 1) + 1\n\n\
fn main():\n    print(go(499))\n    print(go(500))\n";
    if let Some((stdout, panic)) = same("lambda", source) {
        assert_eq!((stdout.as_str(), panic.as_str()), ("499\n", PAST));
    }
}

#[test]
fn a_recursion_through_dyn_counts_the_method_called_through_it() {
    let source = "trait Step:\n    fn step(self, n: int) -> int\n\ntype Down(by: int)\n\n\
impl Step for Down:\n    fn step(self, n: int) -> int:\n        if n == 0:\n            return 0\n        return via(self, n - 1) + self.by\n\n\
fn via(s: dyn Step, n: int) -> int:\n    return s.step(n)\n\n\
fn main():\n    print(via(Down(1), 499))\n    print(via(Down(1), 500))\n";
    // `via` and `step` recurse through each other, `step` called through `dyn`: each level is a
    // call of both, so 500 levels are 1,000 calls.
    if let Some((stdout, panic)) = same("dyn", source) {
        assert_eq!((stdout.as_str(), panic.as_str()), ("499\n", PAST));
    }
}

#[test]
fn a_recursion_through_parallel_meets_the_same_limit() {
    let source = format!(
        "{DOWN}fn spawn(n: int, deep: int) -> int:\n    if n == 0:\n        return parallel([lambda: down(deep)])[0]\n    return spawn(n - 1, deep) + 1\n\n\
fn main():\n    print(spawn(400, 500))\n    print(spawn(600, 500))\n"
    );
    if let Some((stdout, panic)) = same("parallel", &source) {
        assert_eq!((stdout.as_str(), panic.as_str()), ("900\n", PAST));
    }
}

/// `deep(n)`, whose frame holds `locals` integers live across its recursive call.
fn deep(locals: usize) -> String {
    let mut source = String::from("fn deep(n: int) -> int:\n");
    for i in 0..locals {
        source += &format!("    a{i} = n * {i}\n");
    }
    source += "    if n == 0:\n        return 0\n    var total = deep(n - 1)\n";
    for i in 0..locals {
        source += &format!("    total += a{i}\n");
    }
    source + "    return total\n\n"
}

#[test]
fn a_recursion_of_large_frames_meets_the_limit_and_not_the_stack() {
    let source = format!("{}fn main():\n    print(deep(999))\n    print(deep(1000))\n", deep(150));
    if let Some((stdout, panic)) = same("large-frames", &source) {
        assert_eq!((stdout.as_str(), panic.as_str()), (format!("{}\n", 11175_i64 * 499_500).as_str(), PAST));
    }
}

/// What each `test` block of `source` came to, as name, outcome and kind of panic.
fn outcomes(tests: &[Value]) -> Vec<(String, String, Option<String>)> {
    tests
        .iter()
        .map(|t| {
            let field = |key: &str| t[key].as_str().map(ToString::to_string);
            (field("name").unwrap_or_default(), field("outcome").unwrap_or_default(), field("kind"))
        })
        .collect()
}

#[test]
fn a_test_that_panics_deep_leaves_the_next_test_its_whole_limit() {
    let Some(clang) = clang() else { return };
    let source = format!(
        "{DOWN}test \"too deep\":\n    assert down(1000) == 1000\n\ntest \"deep enough\":\n    assert down(999) == 999\n"
    );
    let expected = vec![
        ("too deep".to_string(), "panic".to_string(), Some("RecursionError".to_string())),
        ("deep enough".to_string(), "pass".to_string(), None),
    ];
    for (k, python) in pythons().iter().enumerate() {
        let dir = scratch("py-tests", &format!("recursion-{k}"));
        let path = dir.join("prog.lot");
        std::fs::write(&path, &source).unwrap();
        let module = lotml_py::compile_with(&source, &path, &Interfaces::new()).expect("compiles").module;
        std::fs::write(dir.join("prog.py"), module).unwrap();
        std::fs::write(dir.join("lotml_rt.py"), lotml_py::RUNTIME).unwrap();
        let out = Command::new(&python[0])
            .args(&python[1..])
            .arg("-c")
            .arg("import json, lotml_rt\nprint(json.dumps(lotml_rt.test_modules([['prog', 'prog.lot']])[0]['tests']))")
            .current_dir(&dir)
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .output()
            .expect("Python runs");
        let printed = String::from_utf8_lossy(&out.stdout);
        let tests: Vec<Value> = serde_json::from_str(printed.trim())
            .unwrap_or_else(|e| panic!("{python:?}: {e}: {printed}{}", String::from_utf8_lossy(&out.stderr)));
        assert_eq!(outcomes(&tests), expected, "{python:?}");
    }
    for level in LEVELS {
        let run = build_and_run(&clang, "recursion-tests", &source, level, Build::Tests);
        let last = run.stdout.lines().last().unwrap_or("");
        let tests: Vec<Value> = serde_json::from_str(last).unwrap_or_else(|e| panic!("{e}: {}", run.stdout));
        assert_eq!(outcomes(&tests), expected, "LLVM at {level:?}");
    }
}

/// The C host of the library `rec`: `rec_down(999)` and then `rec_down(n)` for the `n` it is
/// given, on its own thread and on one it starts.
const HOST: &str = r#"#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include "rec.h"
#ifdef _WIN32
#include <windows.h>
#else
#include <pthread.h>
#endif

static int64_t depth;

#ifdef _WIN32
static DWORD WINAPI deep(LPVOID arg) { (void)arg; printf("%lld\n", (long long)rec_down(depth)); fflush(stdout); return 0; }
#else
static void *deep(void *arg) { (void)arg; printf("%lld\n", (long long)rec_down(depth)); fflush(stdout); return NULL; }
#endif

int main(int argc, char **argv) {
    depth = argc > 1 ? atoll(argv[1]) : 999;
    printf("%lld\n", (long long)rec_down(999));
    fflush(stdout);
#ifdef _WIN32
    HANDLE thread = CreateThread(NULL, 0, deep, NULL, 0, NULL);
    WaitForSingleObject(thread, INFINITE);
#else
    pthread_t thread;
    pthread_create(&thread, NULL, deep, NULL);
    pthread_join(thread, NULL);
#endif
    printf("%lld\n", (long long)rec_down(depth));
    return 0;
}
"#;

#[test]
fn an_exported_function_recursing_from_its_host_meets_the_limit_on_the_host_s_thread() {
    let Some(clang) = clang() else { return };
    let dir = scratch("library", "recursion");
    let path = dir.join("rec.lot");
    std::fs::write(&path, DOWN).unwrap();
    let library = lotml_llvm::compile_library(DOWN, &path, &Interfaces::new()).expect("a library");
    lotml_runtime::write(&dir).unwrap();
    std::fs::write(dir.join("rec.h"), &library.header).unwrap();
    let ll = dir.join("rec.ll");
    std::fs::write(&ll, &library.ll).unwrap();
    let lib = dir.join(library_file("rec"));
    clang.build_shared(&ll, &dir, &lib, &library.libraries).unwrap_or_else(|e| panic!("{e}"));
    std::fs::write(dir.join("host.c"), HOST).unwrap();
    let exe = dir.join(if cfg!(windows) { "host.exe" } else { "host" });
    let mut command = Command::new(&clang.program);
    command.args(["-std=c11", "-w", "-o"]).arg(&exe).arg(dir.join("host.c")).arg("-I").arg(&dir);
    if cfg!(windows) {
        command.arg(dir.join("rec.lib"));
    } else {
        command.arg(&lib).arg("-pthread");
    }
    let out = command.output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let run = |n: &str| Command::new(&exe).arg(n).current_dir(Path::new(&dir)).output().expect("the host runs");
    let text = |b: &[u8]| String::from_utf8_lossy(b).replace("\r\n", "\n");
    let within = run("999");
    assert_eq!((text(&within.stdout), within.status.code()), ("999\n999\n999\n".to_string(), Some(0)));
    let past = run("1000");
    assert_eq!(text(&past.stdout), "999\n", "{}", text(&past.stderr));
    assert!(text(&past.stderr).starts_with(&format!("{PAST}\n")), "{}", text(&past.stderr));
    assert_eq!(past.status.code(), Some(101));
}
