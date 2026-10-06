//! `--target c` on `lotml run`, `test` and `build`: the program compiled to C and built, and what
//! the user sees the same as on the Python target (R1.4, R5.1).

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

fn lotml(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lotml")).current_dir(dir).args(args).output().expect("the binary runs")
}

fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("target-c").join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    for (file, text) in files {
        std::fs::write(dir.join(file), text).expect("a scratch file");
    }
    dir
}

const PROGRAM: &str = "fn main():\n    print(\"hi\", 1 + 2, [1.5, 2.0])\n    xs = [1]\n    print(xs[3])\n";

const TESTS: &str = "type Item(name: str, price: f64)\ntype Problem = Missing(name: str) | Empty\n\n\
fn first(items: [Item]) -> Item ! Problem:\n    if len(items) == 0:\n        fail Empty\n    return items[0]\n\n\
fn helper(n: int):\n    assert n > 1, f\"n was {n}\"\n\n\
test \"passes\":\n    assert [Item(\"a\", 1.5)][0].price == 1.5\n\n\
test \"fails with values\":\n    assert [Item(\"a\\n\\\"q\\\"\", 1.0)] == [Item(\"b\", 2.0)]\n\n\
test \"fails shapes\":\n    assert ({3, 1, 2}, {\"k\": None}, (1,)) == ({1}, {\"k\": None}, (2,))\n\n\
test \"fails in a helper\":\n    helper(0)\n\n\
test \"errors\":\n    x = first([])?\n    print(x)\n\n\
test \"panics\":\n    xs = [1, 2]\n    print(xs[5])\n";

#[test]
fn run_on_the_c_target_prints_and_exits_as_python() {
    let dir = scratch("run", &[("prog.lotml", PROGRAM)]);
    let python = lotml(&["run", "prog.lotml"], &dir);
    let c = lotml(&["run", "--target", "c", "prog.lotml"], &dir);
    assert_eq!(c.status.code(), Some(101), "{}", String::from_utf8_lossy(&c.stderr));
    assert_eq!(c.status.code(), python.status.code());
    // Python's standard output is in text mode, which writes `\r\n` on Windows.
    let text = |out: &Output| String::from_utf8_lossy(&out.stdout).replace("\r\n", "\n");
    assert_eq!(text(&c), text(&python));
    assert!(String::from_utf8_lossy(&c.stderr).contains("line 4, in main"));
}

#[test]
fn test_on_the_c_target_reports_as_python() {
    let dir = scratch("test", &[("shop.lotml", TESTS)]);
    let report = |target: &str| -> Value {
        let out = lotml(&["test", "--json", "--target", target, "shop.lotml"], &dir);
        assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stderr));
        serde_json::from_slice(&out.stdout).expect("a JSON report")
    };
    let (python, c) = (report("python"), report("c"));
    assert_eq!(c["summary"], python["summary"]);
    for (p, c) in python["tests"].as_array().unwrap().iter().zip(c["tests"].as_array().unwrap()) {
        assert_eq!(c, p, "the C target reports this test as the Python target does");
    }
    assert_eq!(c["summary"]["passed"], 1);
}

#[test]
fn build_on_the_c_target_writes_an_executable() {
    let dir = scratch("build", &[("prog.lotml", "fn main():\n    print(sum([1, 2, 3]))\n")]);
    let out = lotml(&["build", "--target", "c", "prog.lotml", "-o", "out"], &dir);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let exe = dir.join("out").join(if cfg!(windows) { "prog.exe" } else { "prog" });
    assert!(dir.join("out").join("prog.c").is_file());
    let ran = Command::new(&exe).output().expect("the executable runs");
    assert_eq!(String::from_utf8_lossy(&ran.stdout).replace("\r\n", "\n"), "6\n");
}
