import re
from itertools import pairwise
from pathlib import Path

import pytest
from grammars import (
    IGNORED_SPACES,
    accepts,
    braces_grammar,
    depth,
    indentation_grammar,
    pilot_grammar,
    pilot_incompatibilities,
    skeleton,
    validate,
)

CORPUS = Path(__file__).parent.parent.parent / "tokens" / "corpus"
PROGRAMS = sorted(CORPUS.glob("*/b.x"))


def test_llguidance_rejects_the_pilot_grammar_and_its_declared_indent_tokens():
    error = validate(pilot_grammar())
    assert error != ""


def test_skeleton_drops_comments_and_joins_continuation_lines():
    source = "fn f() -> [int]:  # list\n    # note\n    xs = [\n        1,\n    ]\n    return xs\n"
    assert skeleton(source) == "fn f() -> [int]:\n    xs = [ 1, ]\n    return xs\n"


def test_braces_grammar_is_valid():
    assert validate(braces_grammar()) == ""


@pytest.mark.parametrize("d", [1, 4, 8])
def test_indentation_grammar_is_valid_at_every_depth(d):
    assert validate(indentation_grammar(d)) == ""


@pytest.mark.parametrize("path", PROGRAMS, ids=lambda p: p.parent.name)
def test_both_grammars_accept_every_corpus_program(path):
    program = skeleton(path.read_text(encoding="utf-8"))
    assert accepts(indentation_grammar(depth(program)), program)
    assert accepts(
        braces_grammar(), skeleton(path.read_text(encoding="utf-8"), braces=True)
    )


def test_indentation_grammar_rejects_a_header_without_a_body():
    assert not accepts(indentation_grammar(3), "fn f() -> int:\nreturn 1\n")


def test_indentation_grammar_rejects_a_block_deeper_than_its_bound():
    program = "fn f():\n    if a:\n        if b:\n            pass\n"
    assert depth(program) == 3
    assert accepts(indentation_grammar(3), program)
    assert not accepts(indentation_grammar(2), program)


def test_braces_grammar_rejects_an_unclosed_block():
    assert not accepts(braces_grammar(), "fn f() -> int {\n    return 1\n")


def test_indentation_grammar_grows_linearly_with_depth():
    sizes = [
        len(re.findall(r"^\S", indentation_grammar(d), re.MULTILINE))
        for d in (1, 2, 3, 4)
    ]
    steps = {b - a for a, b in pairwise(sizes)}
    assert len(steps) == 1 and steps.pop() > 0


def test_the_pilot_grammar_needs_priority_and_declare_removed():
    features = [feature for feature, _ in pilot_incompatibilities()]
    assert features == ["terminal priority", "%declare"]


def test_ignored_spaces_defeat_exact_indentation_but_whole_lines_do_not():
    assert accepts(IGNORED_SPACES, "\nif:\n    a")
    assert accepts(IGNORED_SPACES, "\nif:\n        a")
    assert not accepts(indentation_grammar(2), "if x:\n        a\n")
