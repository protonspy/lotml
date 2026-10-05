import pytest

from lotml_harness import ROOT
from lotml_harness.lang.braces import (
    from_braces,
    line_slips,
    shift,
    statement_lines,
    to_braces,
)
from lotml_harness.lang.grammar import parser

CORPUS = sorted((ROOT / "research" / "tokens" / "corpus").glob("*/b.x"))


@pytest.mark.parametrize("path", CORPUS, ids=lambda p: p.parent.name)
def test_braces_round_trip_keeps_the_program(path):
    source = path.read_text(encoding="utf-8")
    braced = to_braces(source)
    back = from_braces(braced)
    assert back is not None
    assert parser("b").parse(back + "\n") == parser("b").parse(source + "\n")


def test_braces_form_reads_like_a_braces_language():
    source = "fn f(x: int) -> int:\n    if x > 0:\n        return 1\n    else:\n        return 2\n"
    assert to_braces(source) == (
        "fn f(x: int) -> int {\n    if x > 0 {\n        return 1\n    } else {\n"
        "        return 2\n    }\n}\n"
    )


def test_whitespace_in_the_braces_form_means_nothing():
    braced = "fn f() {\nif x {\ny = 1\n}\nz = 2\n}\n"
    assert from_braces(braced) == "fn f():\n    if x:\n        y = 1\n    z = 2\n"


def test_unbalanced_braces_are_refused():
    assert from_braces("fn f() {\n    pass\n") is None
    assert from_braces("}\n") is None


def test_strings_and_comments_do_not_open_blocks():
    source = 'fn f() -> str:\n    s = "a {"  # :\n    return s\n'
    assert from_braces(to_braces(source)) == source
    assert statement_lines(source) == [0, 1, 2]


def test_shift_refuses_to_go_left_of_column_zero():
    assert shift("x = 1", [0], -1) is None
    assert shift("x = 1", [0], 1) == "    x = 1"


def test_line_slips_move_each_statement_both_ways():
    slips = list(line_slips("fn f():\n    x = 1\n"))
    assert [(row, delta) for row, delta, _ in slips] == [(0, 1), (1, -1), (1, 1)]
