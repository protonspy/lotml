//! The `lotml` command as a user runs it: exit status, output, files changed.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn lotml(args: &[&str], dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_lotml")).args(args).current_dir(dir).output().expect("the binary runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// A fresh directory for one test, holding `files`.
fn scratch(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("a scratch directory");
    for (file, text) in files {
        std::fs::write(dir.join(file), text).expect("a scratch file");
    }
    dir
}

const CLEAN: &str = "fn f() -> int:\n    return 1\n";
const BROKEN: &str = "fn f() -> int:\n    x = 1\n    x = 2\n    return x\n";

#[test]
fn a_clean_file_exits_zero_and_says_so() {
    let dir = scratch("clean", &[("a.lotml", CLEAN)]);
    let out = lotml(&["check", "a.lotml"], &dir);
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout(&out).contains("a.lotml: no errors"));
}

#[test]
fn errors_exit_one_with_json_root_cause_first() {
    let dir = scratch("json", &[("a.lotml", BROKEN), ("b.lotml", "fn g() -> int:\n    return y\n")]);
    let out = lotml(&["check", "--json", "."], &dir);
    assert_eq!(out.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("JSON");
    assert_eq!(json["version"], 1);
    assert_eq!(json["summary"]["files"], 2);
    // The unknown name is the root cause; the immutable reassignment comes after it.
    assert_eq!(json["diagnostics"][0]["code"], "E0201");
    assert_eq!(json["diagnostics"][1]["code"], "E0301");
}

#[test]
fn sarif_is_available() {
    let dir = scratch("sarif", &[("a.lotml", BROKEN)]);
    let out = lotml(&["check", "--format", "sarif", "a.lotml"], &dir);
    let sarif: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("JSON");
    assert_eq!(sarif["runs"][0]["results"][0]["ruleId"], "E0301");
}

#[test]
fn fix_applies_the_safe_fixes_and_reports_what_is_left() {
    let dir = scratch("fix", &[("a.lotml", BROKEN)]);
    let out = lotml(&["check", "--fix", "a.lotml"], &dir);
    assert_eq!(out.status.code(), Some(0), "{}", stdout(&out));
    let text = std::fs::read_to_string(dir.join("a.lotml")).expect("the file");
    assert_eq!(text, "fn f() -> int:\n    var x = 1\n    x = 2\n    return x\n");
    assert!(stdout(&out).contains("applied 1 fix"));
}

#[test]
fn prefix_gives_a_verdict() {
    let dir = scratch("prefix", &[("a.lotml", "fn f() -> int:\n    return g(")]);
    let out = lotml(&["check", "--prefix", "a.lotml"], &dir);
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout(&out).starts_with("a.lotml: unknown"));
    let dir = scratch("prefix-error", &[("a.lotml", "fn f() -> int:\n    x = 1\n    x = 2\n    return x\n\nfn g")]);
    let out = lotml(&["check", "--prefix", "--json", "a.lotml"], &dir);
    assert_eq!(out.status.code(), Some(1));
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("JSON");
    assert_eq!(json["prefix"][0]["verdict"], "error");
}

/// The variables that point git at another repository. A pre-push hook sets `GIT_DIR`, and a
/// test inheriting it would `git init` and commit into the repository being pushed.
const GIT_LOCATION: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_NAMESPACE",
];

/// `git` in `dir`, for the scratch repository there and no other.
fn git_in(dir: &Path, args: &[&str]) {
    let mut command = Command::new("git");
    for variable in GIT_LOCATION {
        command.env_remove(variable);
    }
    let status = command.args(args).current_dir(dir).output().expect("git runs").status;
    assert!(status.success(), "git {args:?}");
}

/// A scratch git repository holding `files`, committed.
fn repository(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let dir = scratch(name, files);
    git_in(&dir, &["init", "-q"]);
    git_in(&dir, &["-c", "user.name=t", "-c", "user.email=t@t", "add", "."]);
    git_in(&dir, &["-c", "user.name=t", "-c", "user.email=t@t", "commit", "-q", "-m", "start"]);
    dir
}

#[test]
fn since_reads_the_file_s_repository_whatever_git_dir_says() {
    let decoy = repository("since-decoy", &[("a.lotml", "fn f() -> int:\n    return 1\n")]);
    let dir = repository("since-hook", &[("a.lotml", "fn f() -> int:\n    return y\n")]);
    let out = Command::new(env!("CARGO_BIN_EXE_lotml"))
        .args(["check", "--since", "HEAD", "--json", "a.lotml"])
        .current_dir(&dir)
        .env("GIT_DIR", decoy.join(".git"))
        .output()
        .expect("the binary runs");
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("JSON");
    assert_eq!(json["diagnostics"].as_array().map(Vec::len), Some(0), "the error was already there at HEAD");
}

#[test]
fn since_reports_only_what_the_edit_introduced() {
    let dir = repository("since", &[("a.lotml", "fn f() -> int:\n    return y\n")]);
    std::fs::write(dir.join("a.lotml"), "fn f() -> int:\n    return y\n\nfn g() -> int:\n    return z\n")
        .expect("edit");
    let out = lotml(&["check", "--since", "HEAD", "--json", "a.lotml"], &dir);
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("JSON");
    let messages: Vec<&str> =
        json["diagnostics"].as_array().expect("a list").iter().filter_map(|d| d["message"].as_str()).collect();
    assert_eq!(messages, vec!["`z` is not defined"]);
    let out = lotml(&["check", "--since", "no-such-rev", "a.lotml"], &dir);
    assert_eq!(out.status.code(), Some(2));
    let option = lotml(&["check", "--since=--output=leak.txt", "a.lotml"], &dir);
    assert_eq!(option.status.code(), Some(2));
    assert!(!dir.join("leak.txt").exists());
}

#[test]
fn explain_prints_the_code_page() {
    let dir = scratch("explain", &[]);
    let out = lotml(&["explain", "e204"], &dir);
    assert_eq!(out.status.code(), Some(0));
    assert!(stdout(&out).starts_with("E0204: "));
    assert_eq!(lotml(&["explain", "E9999"], &dir).status.code(), Some(2));
}

#[test]
fn fmt_rewrites_and_check_only_reports() {
    let dir = scratch("fmt", &[("a.lotml", "fn f()->int:\n  return 1\n"), ("b.lotml", CLEAN)]);
    let out = lotml(&["fmt", "--check", "."], &dir);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout(&out).contains("a.lotml: not formatted"));
    assert!(!stdout(&out).contains("b.lotml"));
    assert_eq!(lotml(&["fmt", "a.lotml"], &dir).status.code(), Some(0));
    assert_eq!(std::fs::read_to_string(dir.join("a.lotml")).expect("the file"), CLEAN);
    assert_eq!(lotml(&["fmt", "--check", "."], &dir).status.code(), Some(0));
}

const SHOP: &str = "type Item(name: str, price: f64)\ntype Err = Missing(name: str) | Empty\n\nfn total(items: [Item]) -> f64:\n    \"\"\"\n    The sum of the prices.\n\n    Zero for no items.\n    \"\"\"\n    return sum([i.price for i in items])\n\nfn priciest(items: [Item]) -> Item ! Err:\n    if len(items) == 0:\n        fail Empty\n    return max(items, key=lambda i: i.price)\n\nimpl Item:\n    fn label(self) -> str:\n        return f\"{self.name}: {self.price}\"\n\ntest \"total\":\n    assert total([]) == 0.0\n";

#[test]
fn digest_has_types_and_documented_signatures_without_bodies() {
    let dir = scratch("digest", &[("shop.lotml", SHOP)]);
    let out = lotml(&["digest", "shop.lotml"], &dir);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(
        stdout(&out),
        "type Item(name: str, price: f64)\n\ntype Err = Missing(name: str) | Empty\n\n# The sum of the prices.\n#\n# Zero for no items.\nfn total(items: [Item]) -> f64\n\nfn priciest(items: [Item]) -> Item ! Err\n\nimpl Item:\n    fn label(self) -> str\n"
    );
}

#[test]
fn show_gives_the_symbol_and_what_it_uses_once_each() {
    let dir = scratch("show", &[("shop.lotml", SHOP)]);
    let out = stdout(&lotml(&["show", "priciest", "shop.lotml"], &dir));
    assert!(out.contains("fn priciest(items: [Item]) -> Item ! Err:\n    if len(items) == 0:"));
    assert_eq!(out.matches("type Err = Missing(name: str) | Empty").count(), 1);
    assert_eq!(out.matches("type Item(").count(), 1);
    assert!(!out.contains("fn total"));
    let method = stdout(&lotml(&["show", "Item.label", "shop.lotml"], &dir));
    assert!(method.contains("fn label(self) -> str:") && method.contains("type Item("));
    let missing = lotml(&["show", "tota", "shop.lotml"], &dir);
    assert_eq!(missing.status.code(), Some(1));
    assert!(stdout(&missing).contains("the closest are total"));
}

const RUNS: &str = "type E = Bad(code: int)\n\nfn total(xs: [int]) -> int:\n    return sum(xs)\n\nfn risky(n: int) -> int ! E:\n    if n < 0:\n        fail Bad(n)\n    return n\n\nfn main():\n    print(f\"total {total([1, 2, 3])}\")\n\ntest \"sums\":\n    assert total([1, 2]) == 3\n\ntest \"wrong\":\n    assert total([1, 2]) == 4\n\ntest \"passes an error\":\n    n = risky(-2)?\n    assert n == 0\n\ntest \"overflows\":\n    assert total([9223372036854775807, 1]) > 0\n";

#[test]
fn test_reports_each_block_with_the_values_it_saw() {
    let dir = scratch("test", &[("runs.lotml", RUNS)]);
    let out = lotml(&["test", "--json", "runs.lotml"], &dir);
    assert_eq!(out.status.code(), Some(1), "{}", String::from_utf8_lossy(&out.stderr));
    let json: serde_json::Value = serde_json::from_str(&stdout(&out)).expect("JSON");
    let tests = json["tests"].as_array().expect("tests");
    let outcomes: Vec<&str> = tests.iter().filter_map(|t| t["outcome"].as_str()).collect();
    assert_eq!(outcomes, vec!["pass", "fail", "error", "panic"]);
    assert_eq!(
        (tests[1]["left"].as_str(), tests[1]["right"].as_str(), tests[1]["line"].as_u64()),
        (Some("3"), Some("4"), Some(18))
    );
    assert_eq!(tests[1]["file"], "runs.lotml");
    assert_eq!(tests[2]["error"], "Bad(code=-2)");
    assert_eq!(tests[3]["kind"], "Overflow");
    assert_eq!(json["summary"]["passed"], 1);
}

#[test]
fn run_calls_main_and_build_writes_the_modules() {
    let dir = scratch("run", &[("runs.lotml", RUNS)]);
    let out = lotml(&["run", "runs.lotml"], &dir);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(stdout(&out).trim_end(), "total 6");
    let built = lotml(&["build", "-o", "out", "runs.lotml"], &dir);
    assert_eq!(built.status.code(), Some(0));
    assert!(dir.join("out").join("runs_lotml.py").is_file() && dir.join("out").join("lotml_rt.py").is_file());
}

#[test]
fn a_panic_names_the_lotml_line() {
    let dir = scratch("panic", &[("p.lotml", "fn main():\n    xs = [1]\n    print(xs[5])\n")]);
    let out = lotml(&["run", "p.lotml"], &dir);
    assert_eq!(out.status.code(), Some(101));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("panic: IndexError") && stderr.contains("line 3, in main") && stderr.contains("print(xs[5])"),
        "{stderr}"
    );
}

#[test]
fn a_program_that_does_not_check_does_not_run() {
    let dir = scratch("no-run", &[("p.lotml", "fn main():\n    x = 1\n    x = 2\n")]);
    let out = lotml(&["run", "p.lotml"], &dir);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout(&out).contains("E0301"));
}

#[test]
fn a_missing_path_cannot_run() {
    let dir = scratch("missing", &[]);
    assert_eq!(lotml(&["check", "nope.lotml"], &dir).status.code(), Some(2));
}

const SHAPES: &str = "trait Area:\n    fn area(self) -> f64\n\n    fn describe(self) -> str:\n        \"\"\"A line about the shape.\"\"\"\n        return f\"area {self.area()}\"\n\ntype Shape = Circle(r: f64) | Square(side: f64)\ntype Box[T](items: [T] = [])\n\nimpl Area for Shape:\n    fn area(self) -> f64:\n        match self:\n            case Circle(r):\n                return 3.0 * r * r\n            case Square(side):\n                return side * side\n\nfn largest[T: Area](shapes: [T]) -> f64:\n    \"\"\"The largest area.\"\"\"\n    return max([s.area() for s in shapes])\n\nfn unit() -> Shape:\n    return Square(1.0)\n";

#[test]
fn digest_shows_traits_generics_and_several_files() {
    let dir = scratch("digest-more", &[("shapes.lotml", SHAPES), ("shop.lotml", SHOP)]);
    let out = stdout(&lotml(&["digest", "."], &dir));
    assert!(out.contains("# shapes.lotml") && out.contains("# shop.lotml"), "{out}");
    assert!(
        out.contains(
            "trait Area:\n    fn area(self) -> f64\n    # A line about the shape.\n    fn describe(self) -> str\n"
        ),
        "{out}"
    );
    assert!(out.contains("type Box[T](items: [T] = [])"), "{out}");
    assert!(out.contains("# The largest area.\nfn largest[T: Area](shapes: [T]) -> f64\n"), "{out}");
    assert!(out.contains("impl Area for Shape:\n    fn area(self) -> f64\n"), "{out}");
}

#[test]
fn show_finds_variants_traits_and_types_used_through_patterns() {
    let dir = scratch("show-more", &[("shapes.lotml", SHAPES)]);
    let variant = stdout(&lotml(&["show", "Circle", "shapes.lotml"], &dir));
    assert!(variant.starts_with("# shapes.lotml\ntype Shape = Circle(r: f64) | Square(side: f64)"), "{variant}");
    let method = stdout(&lotml(&["show", "Shape.area", "shapes.lotml"], &dir));
    assert!(method.contains("case Circle(r):") && method.contains("# uses Shape"), "{method}");
    let generic = stdout(&lotml(&["show", "largest", "shapes.lotml"], &dir));
    assert!(generic.contains("# uses Area") && generic.contains("trait Area:"), "{generic}");
    let built = stdout(&lotml(&["show", "unit", "shapes.lotml"], &dir));
    assert!(built.contains("# uses Shape") && built.contains("type Shape ="), "{built}");
    let none = lotml(&["show", "zzz", "shapes.lotml"], &dir);
    assert_eq!(stdout(&none).trim_end(), "nothing is called `zzz`");
}

#[test]
fn test_runs_hundreds_of_files_at_once() {
    // The modules once went to Python on its command line, which Windows caps at 32,767 characters.
    let files: Vec<(String, String)> = (0..400)
        .map(|i| (format!("module_with_a_long_name_{i}.lotml"), format!("test \"t{i}\":\n    assert {i} == {i}\n")))
        .collect();
    let named: Vec<(&str, &str)> = files.iter().map(|(n, t)| (n.as_str(), t.as_str())).collect();
    let dir = scratch("many", &named);
    let out = lotml(&["test", "--json", "."], &dir);
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stderr));
    let json: serde_json::Value = serde_json::from_str(stdout(&out).trim()).expect("JSON");
    assert_eq!(json["summary"]["passed"], 400);
}
