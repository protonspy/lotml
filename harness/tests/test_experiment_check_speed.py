"""The check-speed measurement: a large file checked through `lotml-db` from an empty database
and again after a one-line body edit, every label kept beside the others."""

from lotml_harness.experiments import check_speed

LINES = [
    '{"lines": 2254, "functions": 250, "modules": 4, "signatures": 800}',
    '{"step": "parse", "seconds": [0.003, 0.002]}',
    '{"step": "interfaces", "seconds": [0.0015, 0.0014]}',
    '{"step": "check", "seconds": [0.005]}',
    '{"step": "cold", "seconds": [0.008, 0.007]}',
    '{"step": "edit", "seconds": [0.0075, 0.007, 0.009]}',
]


def test_a_record_keeps_each_step_s_fastest_run_and_the_file_s_shape():
    record = check_speed.parse("before", LINES, "2026-10-08T00:00:00+00:00")
    assert record.shape == {"lines": 2254, "functions": 250, "modules": 4, "signatures": 800}
    assert record.seconds["parse"] == 0.002
    assert record.seconds["edit"] == 0.007
    assert record.share("edit", "cold") == 1.0
    assert record.share("edit", "missing") is None


def test_a_record_survives_its_json_file_with_every_run():
    record = check_speed.parse("before", LINES, "2026-10-08T00:00:00+00:00", "Windows AMD64")
    again = check_speed.load(check_speed.dump(record))
    assert again == record
    assert again.runs["edit"] == [0.0075, 0.007, 0.009]


def test_the_report_shows_every_label_in_the_order_measured_against_the_budget():
    before = check_speed.parse("before", LINES, "2026-10-08T00:00:00+00:00")
    after = check_speed.parse(
        "after",
        [LINES[0], '{"step": "cold", "seconds": [0.2]}', '{"step": "edit", "seconds": [0.15]}'],
        "2026-10-09T00:00:00+00:00",
    )
    report = check_speed.markdown([after, before])
    assert report.index("## before") < report.index("## after")
    assert "2254 lines in 250 functions" in report
    assert "| interfaces | 1.40 |" in report
    before_section = report[: report.index("## after")]
    assert "takes 100% of one from an empty database, within the 100 ms budget" in before_section
    assert "Reading the interfaces alone is 20% of the check after the edit." in before_section
    after_section = report[report.index("## after") :]
    assert "takes 75% of one from an empty database, over the 100 ms budget" in after_section
    assert "| parse |" not in after_section and "Reading the interfaces" not in after_section
