"""The benchmarks: each program in LotML and in C, the numeric ones held to 2x C and the
allocation- and sharing-heavy ones reported apart."""

from lotml_harness.experiments import benchmarks


def result(name: str, c: float, lotml: float, same: bool = True) -> benchmarks.Result:
    return benchmarks.Result(name, benchmarks.KINDS[name], c, lotml, same)


def test_every_benchmark_has_a_kind_and_a_c_version_and_nothing_else_does():
    programs = {p.stem for p in benchmarks.BENCHMARKS.glob("*.lotml")}
    baselines = {p.stem for p in benchmarks.BENCHMARKS.glob("*.c")}
    assert programs == baselines == set(benchmarks.KINDS)
    assert set(benchmarks.KINDS.values()) == {"numeric", "allocation", "sharing"}


def test_a_run_is_read_from_the_runner_s_lines_taking_each_program_s_fastest():
    lines = [
        '{"compiler": "clang 23"}',
        '{"name": "fib", "c": [0.3, 0.1, 0.2], "lotml": [0.5, 0.25], '
        '"c_out": "9227465", "lotml_out": "9227465"}',
        '{"name": "sieve", "refused": "`--target llvm` does not compile a list yet"}',
    ]
    compiler, results, refused = benchmarks.parse(lines)
    assert compiler == "clang 23"
    assert results == [benchmarks.Result("fib", "numeric", 0.1, 0.25, True)]
    assert results[0].ratio == 2.5
    assert refused == [("sieve", "`--target llvm` does not compile a list yet")]


def test_the_numeric_criterion_holds_only_when_each_numeric_program_is_within_2x():
    fast = [result("fib", 1.0, 1.5), result("binarytrees", 1.0, 9.0)]
    slow = [result("fib", 1.0, 2.5), result("binarytrees", 1.0, 1.0)]
    assert "within 2x C" in benchmarks.markdown("clang 23", fast)
    report = benchmarks.markdown("clang 23", slow)
    assert "not within 2x C" in report and "`fib`" in report


def test_a_program_that_prints_otherwise_than_its_c_version_fails_the_criterion():
    report = benchmarks.markdown("clang 23", [result("fib", 1.0, 1.0, same=False)])
    assert "not within 2x C" in report
    assert "prints otherwise" in report


def test_a_numeric_program_not_compiled_yet_fails_the_criterion_and_says_why():
    report = benchmarks.markdown("clang 23", [result("fib", 1.0, 1.0)], [("sieve", "a list")])
    assert "## Not compiled yet" in report and "`sieve`: a list" in report
    assert "`sieve` is not compiled yet" in report


def test_against_a_baseline_each_program_says_how_much_longer_it_takes_now():
    before = [result("fib", 0.1, 1.0), result("sieve", 0.1, 2.0), result("cow", 0.1, 1.0)]
    now = [result("fib", 0.1, 1.04), result("sieve", 0.1, 1.9)]
    section = benchmarks.against(before, now, "main")
    assert section.startswith("## Against main")
    assert "| fib | 1.000 | 1.040 | +4.0% |" in section
    assert "| sieve | 2.000 | 1.900 | -5.0% |" in section
    assert "cow" not in section, "a program only one side compiled is left out"
    assert "No program takes more than 5% longer than before." in section


def test_a_program_more_than_5_percent_longer_than_the_baseline_is_named():
    section = benchmarks.against([result("fib", 0.1, 1.0)], [result("fib", 0.1, 1.06)], "main")
    assert "Past 5% longer than before: `fib` takes 6.0% longer." in section
