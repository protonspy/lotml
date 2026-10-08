//! The recursion limit on the LLVM target (specs/recursion-depth R1.2, R1.3, R1.5, R1.6): the
//! compiled program counts its own calls on each thread, inline, and stops with the runtime's
//! `RecursionError` when a call would pass 1,000; a call that returns or fails gives its count
//! back, a test or a task that panics gives its thread back the count it began with, and a
//! `parallel` task starts from its spawner's. At `-O0` and at `-O2`.

mod common;

use common::{Build, build_and_run, clang, run_llvm};
use lotml_llvm::driver::Level;
use serde_json::Value;

const LEVELS: [Level; 2] = [Level::Debug, Level::Release];

const DOWN: &str = "fn down(n: int) -> int:\n    if n == 0:\n        return 0\n    return down(n - 1) + 1\n\n";

fn stopped(stderr: &str) -> bool {
    stderr.starts_with("panic: RecursionError: maximum recursion depth exceeded\n")
}

#[test]
fn a_thousand_calls_in_progress_run_and_one_more_stops_the_program() {
    let Some(clang) = clang() else { return };
    for level in LEVELS {
        let run = run_llvm(&clang, "thousand", &format!("{DOWN}fn main():\n    print(down(999))\n"), level);
        assert_eq!((run.stdout.as_str(), run.code), ("999\n", Some(0)), "{level:?}: {}", run.stderr);
        let run = run_llvm(&clang, "past", &format!("{DOWN}fn main():\n    print(down(1000))\n"), level);
        assert!(stopped(&run.stderr), "{level:?}: {}", run.stderr);
        assert!(run.stderr.contains("line 1, in down"), "{level:?}: named where the call begins: {}", run.stderr);
        assert_eq!(run.code, Some(101));
    }
}

#[test]
fn a_call_that_returns_or_fails_gives_its_count_back() {
    let Some(clang) = clang() else { return };
    let source = format!(
        "{DOWN}fn deny(n: int) -> int ! str:\n    if n == 0:\n        fail \"no\"\n    return deny(n - 1)? + 1\n\n\
fn main():\n    var total = 0\n    for _ in range(5):\n        total += down(999)\n        match deny(999):\n            case Ok(v):\n                total += v\n            case Err(e):\n                total += len(e)\n    print(total)\n"
    );
    for level in LEVELS {
        let run = run_llvm(&clang, "returns", &source, level);
        assert_eq!(run.stdout, "5005\n", "{level:?}: {}", run.stderr);
    }
}

#[test]
fn functions_calling_each_other_meet_the_limit_as_one_calling_itself_does() {
    let Some(clang) = clang() else { return };
    let source = "fn even(n: int) -> bool:\n    if n == 0:\n        return True\n    return odd(n - 1)\n\n\
fn odd(n: int) -> bool:\n    if n == 0:\n        return False\n    return even(n - 1)\n\n\
fn main():\n    print(even(999))\n    print(even(1000))\n";
    for level in LEVELS {
        let run = run_llvm(&clang, "mutual", source, level);
        assert_eq!(run.stdout, "False\n", "{level:?}: {}", run.stderr);
        assert!(stopped(&run.stderr), "{level:?}: {}", run.stderr);
    }
}

#[test]
fn a_test_that_panics_deep_gives_the_next_test_its_whole_limit() {
    let Some(clang) = clang() else { return };
    let source = format!(
        "{DOWN}test \"too deep\":\n    assert down(5000) == 5000\n\ntest \"deep enough\":\n    assert down(999) == 999\n"
    );
    for level in LEVELS {
        let run = build_and_run(&clang, "tests", &source, level, Build::Tests);
        let last = run.stdout.lines().last().unwrap_or("");
        let tests: Vec<Value> = serde_json::from_str(last).unwrap_or_else(|e| panic!("{e}: {}", run.stdout));
        let outcomes: Vec<(&str, &str, Option<&str>)> = tests
            .iter()
            .map(|t| (t["name"].as_str().unwrap_or(""), t["outcome"].as_str().unwrap_or(""), t["kind"].as_str()))
            .collect();
        assert_eq!(
            outcomes,
            vec![("too deep", "panic", Some("RecursionError")), ("deep enough", "pass", None)],
            "{level:?}"
        );
    }
}

#[test]
fn a_parallel_task_starts_from_the_count_of_the_thread_that_started_it() {
    let Some(clang) = clang() else { return };
    let source = format!(
        "{DOWN}fn spawn(n: int, deep: int) -> int:\n    if n == 0:\n        return parallel([lambda: down(deep)])[0]\n    return spawn(n - 1, deep) + 1\n\n\
fn main():\n    print(spawn(400, 500))\n    print(spawn(600, 500))\n"
    );
    for level in LEVELS {
        let run = run_llvm(&clang, "parallel", &source, level);
        assert_eq!(run.stdout, "900\n", "{level:?}: {}", run.stderr);
        assert!(stopped(&run.stderr), "{level:?}: {}", run.stderr);
    }
}

/// `deep(n)`, a function whose frame holds `locals` integers live across its recursive call: a
/// thousand of them pass the 1 MiB a thread is given by default on Windows.
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

/// What `deep(n)` returns with `locals` locals.
fn deep_sum(locals: i64, n: i64) -> i64 {
    (locals * (locals - 1) / 2) * (n * (n + 1) / 2)
}

#[test]
fn the_limit_and_not_the_stack_stops_a_recursion_of_large_frames() {
    let Some(clang) = clang() else { return };
    let locals = 150;
    let expected = deep_sum(locals, 999);
    // The task's lambda is a call that counts, which leaves the task room for 998 more.
    let in_task = deep_sum(locals, 998);
    let main = format!(
        "{}fn main():\n    print(deep(999))\n    print(parallel([lambda: deep(998)])[0])\n    print(deep(1000))\n",
        deep(locals as usize)
    );
    let tests = format!("{}test \"deep\":\n    assert deep(999) == {expected}\n", deep(locals as usize));
    for level in LEVELS {
        let run = run_llvm(&clang, "large-frames", &main, level);
        assert_eq!(run.stdout, format!("{expected}\n{in_task}\n"), "{level:?}: {}", run.stderr);
        assert!(stopped(&run.stderr), "{level:?}: {}", run.stderr);
        assert_eq!(run.code, Some(101), "{level:?}");
        let run = build_and_run(&clang, "large-frames-tests", &tests, level, Build::Tests);
        assert!(run.stdout.contains(r#""outcome": "pass""#), "{level:?}: {}\n{}", run.stdout, run.stderr);
    }
}
