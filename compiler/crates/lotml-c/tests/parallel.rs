//! `parallel` on the C target: each task on a thread of its own, at most 256 at once, the results
//! in order; what the tasks capture marked shared so its counts change atomically; a task that
//! panics stops the program once the others have finished (R4.1, R3.2, R3.3).

mod common;

use common::{parity, run_c_with};

const PARALLEL: &str = include_str!("programs/parallel.lotml");

#[test]
fn parallel_runs_the_tasks_and_keeps_their_order() {
    let run = parity("parallel", PARALLEL);
    assert!(run.stdout.starts_with("[300000, 15, 3]\n300 task 0 ab task 1 cd task 299 cd\n"), "{}", run.stdout);
    assert!(run.c.contains("lt_parallel("), "the tasks run through the runtime's threads");
}

#[test]
fn parallel_frees_what_the_tasks_shared() {
    let run = run_c_with("parallel-free", PARALLEL, true);
    assert!(run.stderr.contains("lotml: 0 cells live at exit"), "{}", run.stderr);
}

#[test]
fn a_task_that_panics_stops_the_program_after_the_others() {
    let source = "fn boom(n: int) -> int:\n    return 10 // n\n\nfn main():\n    print(\"before\")\n    \
                  r = parallel([lambda: boom(1), lambda: boom(0), lambda: boom(2)])\n    print(r)\n";
    let run = parity("parallel-panic", source);
    assert_eq!(run.code, Some(101));
    assert_eq!(run.stdout, "before\n");
    assert!(run.stderr.contains("ZeroDivisionError"), "{}", run.stderr);
}
