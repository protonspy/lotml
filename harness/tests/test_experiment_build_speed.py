"""The build-speed measurement: a small native build split into the runtime's compile, the
program's compile and the link at each level, every label kept beside the others."""

from lotml_harness.experiments import build_speed

LINES = [
    '{"compiler": "clang 23", "program": "fib.lotml"}',
    '{"level": "-O0", "step": "runtime", "seconds": [0.3, 0.24, 0.25]}',
    '{"level": "-O0", "step": "program", "seconds": [0.02]}',
    '{"level": "-O0", "step": "link", "seconds": [0.12]}',
    '{"level": "-O0", "step": "build", "seconds": [0.5, 0.4]}',
    '{"level": "-O2", "step": "runtime", "seconds": [0.8]}',
    '{"level": "-O2", "step": "build", "seconds": [1.0]}',
]


def test_the_benchmark_program_measured_exists():
    assert build_speed.PROGRAM.is_file()


def test_a_record_keeps_each_step_s_fastest_run():
    record = build_speed.parse("before", LINES, "2026-10-07T00:00:00+00:00")
    assert record.compiler == "clang 23" and record.program == "fib.lotml"
    assert record.seconds[("-O0", "runtime")] == 0.24
    assert record.seconds[("-O0", "build")] == 0.4
    assert record.share("-O0") == 0.24 / 0.4
    assert record.share("-O2") == 0.8


def test_a_record_survives_its_json_file():
    record = build_speed.parse("before", LINES, "2026-10-07T00:00:00+00:00", "Windows AMD64")
    assert build_speed.load(build_speed.dump(record)) == record


def test_the_report_shows_every_label_in_the_order_measured_with_the_runtime_s_share():
    before = build_speed.parse("before", LINES, "2026-10-07T00:00:00+00:00")
    after = build_speed.parse(
        "after",
        [
            LINES[0],
            '{"level": "-O0", "step": "cold", "seconds": [0.4]}',
            '{"level": "-O0", "step": "warm", "seconds": [0.1]}',
        ],
        "2026-10-08T00:00:00+00:00",
    )
    report = build_speed.markdown([after, before])
    assert report.index("## before") < report.index("## after")
    assert "| runtime | 0.240 | 0.800 |" in report
    assert "| warm | 0.100 |  |" in report
    assert "60% at `-O0` and 80% at `-O2` of the whole build" in report
    assert "A build with the runtime cached takes 25% at `-O0` of one without." in report
    after_section = report[report.index("## after") :]
    assert "| runtime |" not in after_section and "of the whole build" not in after_section


def test_a_label_names_a_file_in_the_records_and_nothing_outside_them():
    for good in ["before", "after-cache", "v1.2_windows"]:
        assert build_speed.LABEL.fullmatch(good)
    for bad in ["", "../x", "a/b", ".hidden", "a b"]:
        assert not build_speed.LABEL.fullmatch(bad), bad
