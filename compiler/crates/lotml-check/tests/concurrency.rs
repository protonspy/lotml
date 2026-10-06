//! Colorless concurrency (R20): `parallel` runs tasks — functions with no parameters — and
//! waits for them all; no function is marked `async` and none is awaited.

use lotml_check::check_source;

fn codes(source: &str) -> Vec<&'static str> {
    check_source(source).iter().map(|d| d.code).collect()
}

fn clean(source: &str) {
    let found = check_source(source);
    assert!(found.is_empty(), "{source}\n{:?}", found.iter().map(|d| (d.code, &d.message)).collect::<Vec<_>>());
}

const WORK: &str = "fn work(n: int) -> int:\n    return n * n\n\n";

#[test]
fn parallel_returns_each_task_s_result_in_order() {
    clean(&format!("{WORK}fn f(xs: [int]) -> [int]:\n    return parallel([lambda: work(x) for x in xs])\n"));
    clean(&format!(
        "{WORK}fn f() -> int:\n    rs = parallel([lambda: work(1), lambda: work(2)])\n    return rs[0] + rs[1]\n"
    ));
    clean("fn f() -> [int]:\n    return parallel([])\n");
}

#[test]
fn a_task_that_can_fail_gives_a_result_to_handle() {
    clean(
        "type E = Bad\n\nfn may(n: int) -> int ! E:\n    return n\n\n\
         fn f() -> int ! E:\n    var total = 0\n    for r in parallel([lambda: may(1), lambda: may(2)]):\n        total += r?\n    return total\n",
    );
}

#[test]
fn calls_given_where_tasks_are_wanted_are_reported_with_the_fix() {
    let source = format!("{WORK}fn f() -> [int]:\n    return parallel(work(1), work(2))\n");
    let found = check_source(&source);
    assert_eq!(found.iter().map(|d| d.code).collect::<Vec<_>>(), vec!["E0204"]);
    let fix = &found[0].fixes[0];
    assert_eq!(fix.edits[0].replacement, "[lambda: work(1), lambda: work(2)]");
    assert_eq!(fix.applicability, lotml_diag::Applicability::MaybeIncorrect);
    assert_eq!(codes(&format!("{WORK}fn f() -> [int]:\n    return parallel([lambda n: work(n)])\n")), vec!["E0204"]);
    // The habit an open model showed most in the harness: the loop's variable as a parameter.
    let source = format!("{WORK}fn f(xs: [int]) -> [int]:\n    return parallel([lambda x: work(x) for x in xs])\n");
    let found = check_source(&source);
    let (fixed, _) = lotml_diag::apply_fixes(
        &source,
        &[lotml_diag::Diagnostic {
            fixes: found[0]
                .fixes
                .iter()
                .map(|f| lotml_diag::Fix { applicability: lotml_diag::Applicability::MachineApplicable, ..f.clone() })
                .collect(),
            ..found[0].clone()
        }],
    );
    assert!(fixed.contains("parallel([lambda: work(x) for x in xs])"), "{fixed}");
    clean(&fixed);
    assert_eq!(codes(&format!("{WORK}fn f() -> [int]:\n    return parallel(3)\n")), vec!["E0204"]);
}

#[test]
fn async_and_await_are_python_s_and_their_fix_removes_them() {
    let source = "async fn fetch(n: int) -> int:\n    return n\n\nfn f() -> int:\n    return await fetch(1)\n";
    let found = check_source(source);
    assert_eq!(found.iter().map(|d| d.code).collect::<Vec<_>>(), vec!["E0112", "E0112"]);
    let (fixed, applied) = lotml_diag::apply_fixes(source, &found);
    assert_eq!(applied, 2);
    assert_eq!(fixed, "fn fetch(n: int) -> int:\n    return n\n\nfn f() -> int:\n    return fetch(1)\n");
    clean(&fixed);
}

#[test]
fn a_name_await_is_still_a_name() {
    clean("fn f(await: int) -> int:\n    return await + 1\n");
}
