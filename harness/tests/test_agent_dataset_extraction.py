"""Repairs extracted from one trace, by check and by test (specs/trace-dataset/ R3.2-R3.6, R3.9)."""

from lotml_harness.agent.dataset import block_repairs, check_repairs

BROKEN = "fn f() -> int:\n    return x\n"
FIXED = "fn f() -> int:\n    return 1\n"
OTHER = "fn g() -> int:\n    return 2\n"


def error(file: str) -> dict:
    return {"file": file, "code": "E0201", "severity": "error", "message": "`x` is not defined"}


def check(files: dict, errors: list[dict], paths=(), status="success", after=None, **extra) -> dict:
    report = {"diagnostics": errors, "summary": {"errors": len(errors)}}
    record = {
        "paths": list(paths),
        "arguments": {},
        "before": files,
        "after": after if after is not None else files,
        "report": report,
        "status": status,
        "truncated": False,
    }
    return record | extra


def trace(checks=(), tests=(), outcome="fail") -> dict:
    return {
        "row": {"task": "humaneval-0", "model": "m", "arm": "agents", "outcome": outcome,
                "compiler": "lotml 0.1.0", "providers": {"P": 1}},
        "messages": [{"type": "human", "data": {"content": "Write f."}}],
        "checks": list(checks),
        "tests": list(tests),
    }  # fmt: skip


def test_a_check_with_errors_closed_by_a_clean_check_is_a_repair():
    found = check_repairs(
        trace([check({"a.lotml": BROKEN}, [error("a.lotml")]), check({"a.lotml": FIXED}, [])])
    )
    [repair] = found
    assert (repair["path"], repair["before"], repair["after"]) == ("a.lotml", BROKEN, FIXED)
    assert repair["diagnostics"] == [error("a.lotml")]
    assert repair["prompt"] == "Write f."
    assert repair["changed"] == [2]
    assert repair["meta"] == {
        "task": "humaneval-0",
        "source": "humaneval-original",
        "model": "m",
        "arm": "agents",
        "outcome": "fail",
        "compiler": "lotml 0.1.0",
        "origin": "real",
    }


def test_the_last_failing_check_before_the_clean_one_is_the_failing_state():
    second = "fn f() -> int:\n    return y\n"
    found = check_repairs(trace([
        check({"a.lotml": BROKEN}, [error("a.lotml")]),
        check({"a.lotml": second}, [error("a.lotml")]),
        check({"a.lotml": FIXED}, []),
    ]))  # fmt: skip
    assert [r["before"] for r in found] == [second]


def test_a_file_still_failing_at_the_end_gives_nothing():
    assert check_repairs(trace([check({"a.lotml": BROKEN}, [error("a.lotml")])])) == []


def test_only_a_check_that_judged_the_file_closes_its_repair():
    files = {"a.lotml": BROKEN, "src/b.lotml": OTHER}
    later = {"a.lotml": FIXED, "src/b.lotml": OTHER}
    other_only = check(later, [], paths=["src"])
    assert check_repairs(trace([check(files, [error("a.lotml")]), other_only])) == []
    closing = check(later, [], paths=["."])
    assert len(check_repairs(trace([check(files, [error("a.lotml")]), closing]))) == 1


def test_a_check_that_failed_saw_its_files_change_or_gave_no_json_counts_neither_way():
    opened = check({"a.lotml": BROKEN}, [error("a.lotml")])
    for spoiled in (
        check({"a.lotml": FIXED}, [], status="error"),
        check({"a.lotml": FIXED}, [], after={"a.lotml": BROKEN}),
        check({"a.lotml": FIXED}, [], report=None),
        check({"a.lotml": FIXED}, [], truncated=True),
    ):
        assert check_repairs(trace([opened, spoiled])) == []


def test_an_identical_repair_is_written_once_per_task():
    pair = [check({"a.lotml": BROKEN}, [error("a.lotml")]), check({"a.lotml": FIXED}, [])]
    assert len(check_repairs(trace(pair + pair))) == 1


def test_a_failing_block_closed_by_a_passing_run_of_it_is_a_test_repair():
    failing = {
        "file": "~/tmp/ws/a.lotml",
        "name": "adds",
        "outcome": "fail",
        "left": "-1",
        "right": "3",
    }
    passing = {"file": "~/tmp/ws/a.lotml", "name": "adds", "outcome": "pass"}

    def tested(files, rows, **extra):
        record = check(files, [], **extra)
        record["report"] = {"tests": rows}
        return record

    found = block_repairs(
        trace(tests=[tested({"a.lotml": BROKEN}, [failing]), tested({"a.lotml": FIXED}, [passing])])
    )
    [repair] = found
    assert (repair["path"], repair["before"], repair["after"]) == ("a.lotml", BROKEN, FIXED)
    assert repair["failing"] == failing
    assert repair["diagnostics"] is None
    assert repair["meta"]["origin"] == "real"
    unclosed = trace(
        tests=[tested({"a.lotml": BROKEN}, [failing]), tested({"a.lotml": FIXED}, [failing])]
    )
    assert block_repairs(unclosed) == []
