//! Perceus-style reuse on the C target: a `match` arm taking apart a value it alone holds builds
//! its new value in that value's memory (R3.4).

mod common;

use common::{parity, run_c_with};

const REUSE: &str = include_str!("programs/reuse.lotml");

/// How many cells a counting build of `source` allocated, and how many were live at its end.
fn cells(name: &str, source: &str) -> (u64, u64) {
    let run = run_c_with(name, source, true);
    let number = |label: &str| -> u64 {
        let line = run.stderr.lines().find(|l| l.contains(label)).unwrap_or_else(|| panic!("{}", run.stderr));
        line.split_whitespace().nth(1).and_then(|n| n.parse().ok()).unwrap_or_else(|| panic!("{line}"))
    };
    (number("cells allocated"), number("cells live at exit"))
}

#[test]
fn reuse_gives_the_same_results() {
    let run = parity("reuse", REUSE);
    assert_eq!(run.stdout, "20300\n15 20 True\n");
}

#[test]
fn a_unique_chain_is_mapped_in_its_own_memory() {
    let (allocated, live) = cells("reuse-count", REUSE);
    assert_eq!(live, 0);
    // build(200) allocates 200 links; bump reuses them all. The rest of main allocates 15:
    // build(5) twice and bump(xs), whose xs is still used after it and so is not unique.
    assert!(allocated <= 230, "{allocated} cells allocated: bump did not reuse the links it took apart");
    assert!(allocated >= 215, "{allocated} cells allocated: bump(xs) reused cells xs still holds");
}
