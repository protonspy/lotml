//! Uniqueness checked once before a loop that stores into a list: the list is still copied before
//! it changes when another binding holds it, before the loop or from inside it (R3.5).

mod common;

use common::{parity, run_c_with};

const PROGRAM: &str = "fn fill(n: int) -> [[int]]:
    base = [0] * n
    var row = base
    for i in range(n):
        row[i] = i
    var rows: [[int]] = []
    var grid = [0] * n
    for i in range(n):
        grid[i] = i * 10
        rows.append(grid)
    var counts = [0] * 3
    var j = 0
    while j < 9:
        counts[j % 3] += 1
        if counts[0] > 1:
            counts[1] = len(counts) + counts[0]
        j += 1
    return [base, row, counts] + rows

fn main():
    print(fill(3))
";

#[test]
fn a_list_changed_in_a_loop_is_still_copied_when_shared() {
    let run = parity("hoist", PROGRAM);
    assert_eq!(run.stdout, "[[0, 0, 0], [0, 1, 2], [3, 6, 3], [0, 0, 0], [0, 10, 0], [0, 10, 20]]\n", "{}", run.c);
    assert!(run.c.contains("lt_list_unique(&"), "the check runs before the loop");
}

#[test]
fn a_list_changed_in_a_loop_frees_what_it_takes() {
    let run = run_c_with("hoist-free", PROGRAM, true);
    assert!(run.stderr.contains("lotml: 0 cells live at exit"), "{}", run.stderr);
}
