//! Records, sum types, `match`, optionals and results on the C target (R1.2, R1.3).

mod common;

use common::{parity, run_c_with};

const ADT: &str = include_str!("programs/adt.lotml");

#[test]
fn records_variants_options_and_results_behave_as_in_python() {
    let run = parity("adt", ADT);
    assert!(run.stdout.starts_with("Point(x=1.0, y=2.5) Point(x=5.0, y=2.5) 3.5 True True\n"), "{}", run.stdout);
    assert!(run.stdout.contains("Err(error=NotNumber(text='x1'))"), "{}", run.stdout);
}

#[test]
fn records_and_variants_are_freed() {
    let run = run_c_with("adt-free", ADT, true);
    assert!(run.stderr.contains("lotml: 0 cells live at exit"), "{}", run.stderr);
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
    let run = parity(
        "main-error",
        "type Bad = Oops(text: str)\n\nfn check(s: str) -> int ! Bad:\n    if s == \"\":\n        fail Oops(\"empty\")\n    return 1\n\n\
         fn main() -> None ! Bad:\n    print(check(\"x\")?)\n    print(check(\"\")?)\n    print(\"never\")\n",
    );
    assert_eq!(run.code, Some(1));
    assert_eq!(run.stdout, "1\n");
}
