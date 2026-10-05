import textwrap
from pathlib import Path

import pytest
from slips import (
    Baseline,
    brace_slips,
    classify,
    consistent,
    from_braces,
    hunk_slips,
    line_slips,
    shift,
    statement_lines,
    to_braces,
)
from transpile import parser

CORPUS = Path(__file__).parent.parent.parent / "tokens" / "corpus"

TOTAL = textwrap.dedent("""\
    fn total(xs: [int]) -> int:
        var t = 0
        for x in xs:
            t += x
        return t

    test "total":
        assert total([1, 2]) == 3
""")

SINGLE = TOTAL.replace("total([1, 2]) == 3", "total([5]) == 5")


def lines_of(source: str, text: str) -> int:
    return next(i for i, line in enumerate(source.splitlines()) if line.strip() == text)


def test_statement_lines_skip_blanks_comments_and_continuations():
    source = (
        "fn f() -> [int]:\n    # note\n    xs = [\n        1,\n    ]\n\n    return xs\n"
    )
    assert statement_lines(source) == [0, 2, 6]


def test_strings_do_not_open_brackets():
    source = 'fn f() -> str:\n    s = "(["\n    return s\n'
    assert statement_lines(source) == [0, 1, 2]


def test_shift_refuses_to_go_left_of_column_zero():
    assert shift(TOTAL, [0], -1) is None
    assert shift(TOTAL, [1], 1).splitlines()[1] == "        var t = 0"


def test_dedenting_the_return_out_of_the_function_is_rejected():
    row = lines_of(TOTAL, "return t")
    assert classify(Baseline.of(TOTAL), shift(TOTAL, [row], -1)) == "rejected: syntax"


def test_emptying_a_loop_body_is_rejected():
    row = lines_of(TOTAL, "t += x")
    assert classify(Baseline.of(TOTAL), shift(TOTAL, [row], -1)) == "rejected: syntax"


def test_return_slipping_into_the_loop_is_caught_by_the_tests_or_not():
    row = lines_of(TOTAL, "return t")
    assert (
        classify(Baseline.of(TOTAL), shift(TOTAL, [row], 1))
        == "rejected: missing return"
    )
    function = TOTAL.replace(
        "-> int:", "-> int:\n    if len(xs) == 0:\n        return 0", 1
    )
    row = lines_of(function, "return t")
    assert (
        classify(Baseline.of(function), shift(function, [row], 1))
        == "rejected: missing return"
    )


def test_a_silent_change_the_tests_miss():
    source = SINGLE.replace("    return t\n", "    t = t * 1\n    return t\n")
    row = lines_of(source, "t = t * 1")
    mutant = shift(source, [row], 1)
    assert classify(Baseline.of(source), mutant) == "silent: tests pass"


def test_a_silent_change_the_tests_catch():
    source = TOTAL.replace("    return t\n", "    t = t * 2\n    return t\n").replace(
        "== 3", "== 6"
    )
    row = lines_of(source, "t = t * 2")
    assert (
        classify(Baseline.of(source), shift(source, [row], 1)) == "silent: tests catch"
    )


def test_line_and_hunk_slips_enumerate_both_directions():
    rows = {(row, delta) for row, delta, _ in line_slips(TOTAL)}
    assert (lines_of(TOTAL, "return t"), 1) in rows
    assert (lines_of(TOTAL, "return t"), -1) in rows
    hunks = {row for row, _, _ in hunk_slips(TOTAL)}
    assert lines_of(TOTAL, "for x in xs:") in hunks


@pytest.mark.parametrize(
    "path", sorted(CORPUS.glob("*/b.x")), ids=lambda p: p.parent.name
)
def test_braces_round_trip_keeps_the_program(path):
    source = path.read_text(encoding="utf-8")
    braces = to_braces(source)
    assert consistent(braces)
    assert parser("b").parse(from_braces(braces)) == parser("b").parse(source)


def test_braces_form_reads_like_a_braces_language():
    braces = to_braces(TOTAL)
    assert "for x in xs {" in braces
    assert braces.splitlines()[4] == "    }"


def test_braces_ignore_whitespace_slips_but_the_redundant_check_does_not():
    braces = to_braces(TOTAL)
    row = lines_of(braces, "t += x")
    slipped = shift(braces, [row], -1)
    assert parser("b").parse(from_braces(slipped)) == parser("b").parse(TOTAL)
    assert not consistent(slipped)


def test_brace_slips_cover_moves_drops_and_duplicates():
    kinds = {kind for kind, _ in brace_slips(to_braces(TOTAL))}
    assert kinds == {"brace up", "brace down", "brace dropped", "brace doubled"}
    for kind, mutant in brace_slips(to_braces(TOTAL)):
        assert not consistent(mutant), kind
        if kind in ("brace dropped", "brace doubled"):
            assert from_braces(mutant) is None


def test_an_indented_first_line_is_rejected_as_python_does():
    assert classify(Baseline.of(TOTAL), shift(TOTAL, [0], 1)) == "rejected: syntax"


def test_a_comment_first_line_may_be_indented():
    source = "# heading\n" + TOTAL
    assert classify(Baseline.of(source), shift(source, [0], 1)) == "unchanged"
    braces = to_braces(source)
    assert (
        classify(Baseline.of(source), from_braces(shift(braces, [0], 1))) == "unchanged"
    )


def test_else_is_cuddled_with_the_closing_brace_and_reads_back():
    source = "fn f(x: int) -> int:\n    if x > 0:\n        return 1\n    else:\n        return 0\n"
    braces = to_braces(source)
    assert "    } else {" in braces.splitlines()
    assert consistent(braces)
    assert parser("b").parse(from_braces(braces)) == parser("b").parse(source)
