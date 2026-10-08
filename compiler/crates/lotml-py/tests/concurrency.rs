//! Colorless concurrency on the Python target (R20): `parallel` runs each task on a thread of
//! its own and waits for them all, so a call that blocks holds up only its task; a task sees
//! copies of the values it captured, never a value another can change.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use lotml_check::{Interfaces, interface};
use lotml_py::{RUNTIME, compile_with, python};

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("concurrency").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    dir
}

/// Compile `source` with the interface of Python's `time` and run its `main` as `lotml run`
/// does, the program unwrapped.
fn run(name: &str, source: &str) -> Output {
    let dir = scratch(name);
    let path = dir.join("prog.lotml");
    std::fs::write(&path, source).unwrap();
    let time = interface("fn sleep(secs: f64) -> None ! PyError\nfn monotonic() -> f64 ! PyError\n").0;
    let interfaces = Interfaces::from([("py.time".to_string(), time)]);
    let compiled = compile_with(source, &path, &interfaces).unwrap_or_else(|d| panic!("{source}\n{d:#?}"));
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
fn parallel_gives_the_results_in_the_order_of_the_tasks() {
    let out = run(
        "order",
        "fn work(n: int) -> int:\n    return n * n\n\nfn main():\n    print(parallel([lambda: work(x) for x in [1, 2, 3, 4]]))\n    print(parallel([]))\n",
    );
    assert_eq!(stdout(&out), "[1, 4, 9, 16]\n[]\n", "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn a_lambda_captures_the_value_its_name_had_when_it_was_made() {
    // Python reads a closure's variable when it is called; lotml captures a copy (R05).
    let out = run(
        "capture",
        "fn main():\n    fs = [lambda: x * 10 for x in [1, 2, 3]]\n    print([f() for f in fs])\n\
         \x20   var gs = []\n    for y in [4, 5]:\n        z = y + 1\n        gs.append(lambda: z)\n    print([g() for g in gs])\n\
         \x20   var total = 1\n    h = lambda: total\n    total = 2\n    print(h())\n",
    );
    assert_eq!(stdout(&out), "[10, 20, 30]\n[5, 6]\n1\n", "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn a_task_that_blocks_holds_up_only_itself() {
    // Four tasks that each sleep 0.4 s finish in about 0.4 s, not 1.6 s.
    let out = run(
        "blocking",
        "import py.time\n\nfn nap() -> int ! PyError:\n    py.time.sleep(0.4)?\n    return 1\n\n\
         fn main() -> None ! PyError:\n    start = py.time.monotonic()?\n\
         \x20   results = parallel([lambda: nap(), lambda: nap(), lambda: nap(), lambda: nap()])\n\
         \x20   elapsed = py.time.monotonic()? - start\n    var done = 0\n    for r in results:\n        done += r?\n\
         \x20   print(done)\n    print(elapsed < 1.2)\n",
    );
    assert_eq!(stdout(&out), "4\nTrue\n", "{}", String::from_utf8_lossy(&out.stderr));
}

#[test]
fn a_task_that_panics_stops_the_program_after_the_others() {
    let out = run(
        "panic",
        "fn boom(n: int) -> int:\n    return 10 // n\n\nfn main():\n    print(parallel([lambda: boom(1), lambda: boom(0)]))\n",
    );
    assert_eq!(out.status.code(), Some(101));
    assert!(String::from_utf8_lossy(&out.stderr).contains("ZeroDivisionError"));
}
