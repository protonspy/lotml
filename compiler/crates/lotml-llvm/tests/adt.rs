//! Records, sum types, `match`, optionals and results on the LLVM target, and the reuse of a cell a
//! `match` arm takes apart (specs/llvm-parity R1.2, R1.3, R3.1).

mod common;

use common::{Build, build_and_run, clang, frees_everything, parity};
use lotml_llvm::driver::Level;

const ADT: &str = include_str!("programs/adt.lotml");
const REUSE: &str = include_str!("programs/reuse.lotml");
const CORPUS: &str = include_str!("programs/corpus.lotml");

#[test]
fn records_variants_options_and_results_behave_as_in_python() {
    let Some([run, _]) = parity("adt", ADT) else { return };
    assert!(run.stdout.starts_with("Point(x=1.0, y=2.5) Point(x=5.0, y=2.5) 3.5 True True\n"), "{}", run.stdout);
    assert!(run.stdout.contains("Err(error=NotNumber(text='x1'))"), "{}", run.stdout);
}

#[test]
fn records_and_variants_are_freed() {
    frees_everything("adt-free", ADT);
}

#[test]
fn a_record_field_changes_through_a_var_and_its_copies_do_not() {
    parity(
        "record-fields",
        "type Point(x: int, y: int)\ntype Box(label: str, at: Point)\n\n\
         fn main():\n    var pts = [Point(1, 2), Point(3, 4)]\n    copy = pts\n    pts[0].x = 10\n    \
         var b = Box(\"a\", Point(0, 0))\n    c = b\n    b.at.y = 7\n    b.label = b.label + \"!\"\n    \
         print(pts, copy, b, c, b.at.y, c.at.y)\n",
    );
}

#[test]
fn a_failed_main_reports_its_error_and_exits_with_one() {
    let Some(runs) = parity(
        "main-error",
        "type Bad = Oops(text: str)\n\nfn check(s: str) -> int ! Bad:\n    if s == \"\":\n        fail Oops(\"empty\")\n    return 1\n\n\
         fn main() -> None ! Bad:\n    print(check(\"x\")?)\n    print(check(\"\")?)\n    print(\"never\")\n",
    ) else {
        return;
    };
    for run in &runs {
        assert_eq!(run.code, Some(1));
        assert_eq!(run.stdout, "1\n");
        assert!(run.stderr.contains("Oops(text='empty')"), "{}", run.stderr);
    }
}

#[test]
fn what_the_corpus_met_behaves_as_in_python() {
    let Some([run, _]) = parity("corpus", CORPUS) else { return };
    assert!(run.stdout.starts_with("[1, 2, 3, 4, 5, 6]\n"), "{}", run.stdout);
    frees_everything("corpus-free", CORPUS);
}

#[test]
fn reuse_gives_the_same_results() {
    let Some([run, _]) = parity("reuse", REUSE) else { return };
    assert_eq!(run.stdout, "20300\n15 20 True\n");
}

#[test]
fn a_unique_chain_is_mapped_in_its_own_memory() {
    let Some(clang) = clang() else { return };
    let run = build_and_run(&clang, "reuse-count", REUSE, Level::Release, Build::Counting);
    let number = |label: &str| -> u64 {
        let line = run.stderr.lines().find(|l| l.contains(label)).unwrap_or_else(|| panic!("{}", run.stderr));
        line.split_whitespace().nth(1).and_then(|n| n.parse().ok()).unwrap_or_else(|| panic!("{line}"))
    };
    let (allocated, live) = (number("cells allocated"), number("cells live at exit"));
    assert_eq!(live, 0);
    // build(200) allocates 200 links; bump reuses them all. The rest of main allocates 15:
    // build(5) twice and bump(xs), whose xs is still used after it and so is not unique.
    assert!(allocated <= 230, "{allocated} cells allocated: bump did not reuse the links it took apart");
    assert!(allocated >= 215, "{allocated} cells allocated: bump(xs) reused cells xs still holds");
}
