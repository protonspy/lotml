"""The repair record the exporter and the seeded failures share (trace-dataset R3.2, R3.9)."""

from lotml_harness.agent.dataset import changed_lines, repair


def test_changed_lines_are_the_failing_file_s_lines_the_repair_touched():
    before = "a\nb\nc\nd\n"
    assert changed_lines(before, "a\nB\nc\nd\n") == [2]
    assert changed_lines(before, "a\nc\nd\n") == [2]
    assert changed_lines(before, "a\nb\nc\nd\ne\n") == [4], "an insertion names the line it follows"
    assert changed_lines(before, "z\na\nb\nc\nd\n") == [1], "at the start, line 1"
    assert changed_lines(before, before) == []


def test_a_repair_carries_both_files_what_was_said_and_the_changed_lines():
    record = repair(
        "Fix it.",
        "a.lotml",
        "x\ny\n",
        "x\nz\n",
        {"origin": "seeded"},
        failing={"name": "t"},
    )
    assert record == {
        "prompt": "Fix it.",
        "path": "a.lotml",
        "before": "x\ny\n",
        "after": "x\nz\n",
        "diagnostics": None,
        "failing": {"name": "t"},
        "changed": [2],
        "meta": {"origin": "seeded"},
    }
