//! The recursion limit on the Python target (specs/recursion-depth R1.2, R1.3, R1.5, R1.6): the
//! program counts its own calls on each thread and stops with its own `RecursionError` when a call
//! would pass 1,000, whatever CPython's limit; a call that returns, fails or ends in a caught panic
//! gives its count back, and a `parallel` task starts from its spawner's.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use lotml_check::Interfaces;
use lotml_py::{RUNTIME, compile_with, python};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("depth").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

/// Compile `source` into `prog.py` beside the runtime, and run `script` there. CPython's own limit
/// is moved out of the way first, as `lotml run` and `lotml test` do (R2.2).
fn run_script(name: &str, source: &str, script: &str) -> Output {
    let dir = scratch(name);
    let path = dir.join("prog.lot");
    std::fs::write(&path, source).unwrap();
    let compiled = compile_with(source, &path, &Interfaces::new()).unwrap_or_else(|d| panic!("{source}\n{d:#?}"));
    std::fs::write(dir.join("prog.py"), compiled.module).unwrap();
    std::fs::write(dir.join("lotml_rt.py"), RUNTIME).unwrap();
    let python = python().expect("a Python interpreter");
    Command::new(&python[0])
        .args(&python[1..])
        .arg("-c")
        .arg(format!("import sys, threading\nsys.setrecursionlimit(20000)\nthreading.stack_size(64 << 20)\n{script}"))
        .current_dir(&dir)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .env("PYTHONIOENCODING", "utf-8")
        .output()
        .expect("Python runs")
}

/// Run `source`'s `main` as `lotml run` does, on a thread with room for the limit.
fn run(name: &str, source: &str) -> Output {
    run_script(
        name,
        source,
        "import lotml_rt\nout = []\nt = threading.Thread(target=lambda: out.append(lotml_rt.main('prog')))\nt.start()\nt.join()\nsys.exit(out[0])",
    )
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).replace("\r\n", "\n")
}

const DOWN: &str = "fn down(n: int) -> int:\n    if n == 0:\n        return 0\n    return down(n - 1) + 1\n\n";

#[test]
fn a_thousand_calls_in_progress_run_and_one_more_stops_the_program() {
    let out = run("thousand", &format!("{DOWN}fn main():\n    print(down(999))\n"));
    assert_eq!(text(&out.stdout), "999\n", "{}", text(&out.stderr));
    assert_eq!(out.status.code(), Some(0));
    let out = run("past", &format!("{DOWN}fn main():\n    print(down(1000))\n"));
    let stderr = text(&out.stderr);
    assert!(stderr.starts_with("panic: RecursionError: maximum recursion depth exceeded\n"), "{stderr}");
    assert!(stderr.contains("line 1, in down"), "named where the call that would pass it begins: {stderr}");
    assert_eq!(out.status.code(), Some(101));
}

#[test]
fn the_limit_holds_whatever_cpython_allows() {
    let source = format!("{DOWN}fn main():\n    print(down(5000))\n");
    let out = run("cpython-allows", &source);
    assert!(
        text(&out.stderr).starts_with("panic: RecursionError: maximum recursion depth exceeded\n"),
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn a_call_that_returns_or_fails_gives_its_count_back() {
    let source = format!(
        "{DOWN}fn deny(n: int) -> int ! str:\n    if n == 0:\n        fail \"no\"\n    return deny(n - 1)? + 1\n\n\
fn main():\n    var total = 0\n    for _ in range(5):\n        total += down(999)\n        match deny(999):\n            case Ok(v):\n                total += v\n            case Err(e):\n                total += len(e)\n    print(total)\n"
    );
    let out = run("returns", &source);
    assert_eq!(text(&out.stdout), "5005\n", "{}", text(&out.stderr));
}

#[test]
fn functions_calling_each_other_meet_the_limit_as_one_calling_itself_does() {
    let source = "fn even(n: int) -> bool:\n    if n == 0:\n        return True\n    return odd(n - 1)\n\n\
fn odd(n: int) -> bool:\n    if n == 0:\n        return False\n    return even(n - 1)\n\n\
fn main():\n    print(even(999))\n    print(even(1000))\n";
    let out = run("mutual", source);
    assert_eq!(text(&out.stdout), "False\n");
    assert!(text(&out.stderr).starts_with("panic: RecursionError: "), "{}", text(&out.stderr));
}

#[test]
fn a_test_that_panics_deep_gives_the_next_test_its_whole_limit() {
    let source = format!(
        "{DOWN}test \"too deep\":\n    assert down(5000) == 5000\n\ntest \"deep enough\":\n    assert down(999) == 999\n"
    );
    let out = run_script(
        "tests",
        &source,
        "import json, lotml_rt, prog\nprint(json.dumps([(r['name'], r['outcome'], r.get('kind')) for r in lotml_rt.run_tests(vars(prog), prog.__lotml__)]))",
    );
    assert_eq!(
        text(&out.stdout).trim(),
        r#"[["too deep", "panic", "RecursionError"], ["deep enough", "pass", null]]"#,
        "{}",
        text(&out.stderr)
    );
}

#[test]
fn a_parallel_task_starts_from_the_count_of_the_thread_that_started_it() {
    let source = format!(
        "{DOWN}fn spawn(n: int, deep: int) -> int:\n    if n == 0:\n        return parallel([lambda: down(deep)])[0]\n    return spawn(n - 1, deep) + 1\n\n\
fn main():\n    print(spawn(400, 500))\n    print(spawn(600, 500))\n"
    );
    let out = run("parallel", &source);
    let stderr = text(&out.stderr);
    assert_eq!(text(&out.stdout), "900\n", "{stderr}");
    assert!(stderr.starts_with("panic: RecursionError: maximum recursion depth exceeded\n"), "{stderr}");
}
