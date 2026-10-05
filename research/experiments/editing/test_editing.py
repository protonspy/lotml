import pytest
from edit_tasks import TASKS
from editing import (
    apply,
    outcome,
    parse_blocks,
    program,
    reference_edit,
    render_blocks,
    spec,
)

SOURCE = "fn f(x: int) -> int:\n    if x > 0:\n        return 1\n    return 0\n"


def test_blocks_round_trip_through_the_text_format():
    blocks = [("a\n", "b\n"), ("c\n", "")]
    assert parse_blocks(render_blocks(blocks)) == blocks


def test_strict_apply_needs_an_exact_unique_match():
    edited, reason = apply(SOURCE, [("        return 1\n", "        return 2\n")])
    assert reason is None and "return 2" in edited
    assert (
        apply(SOURCE, [("    return 1\n", "    return 2\n")])[1]
        == "not found: whitespace"
    )
    assert apply(SOURCE, [("return 3\n", "x\n")])[1] == "not found"
    assert apply("a\nb\na\n", [("a\n", "c\n")])[1] == "ambiguous"


def test_tolerant_apply_reindents_the_replacement_relative_to_the_match():
    blocks = [("if x > 0:\n    return 1\n", "if x > 1:\n    return 1\n")]
    edited, reason = apply(SOURCE, blocks, tolerant=True)
    assert reason is None
    assert "    if x > 1:\n        return 1\n" in edited


def test_braces_form_and_spec_say_braces():
    assert "{" in program("04-binary-search", "braces").splitlines()[0]
    assert "`{`" in spec("braces") and "lotml" in spec("braces")
    assert "`.lotml`" in spec("indented")


@pytest.mark.parametrize("task", TASKS, ids=lambda t: t.program)
@pytest.mark.parametrize("form", ["indented", "braces"])
def test_every_hidden_test_fails_unedited_and_passes_with_the_reference(task, form):
    assert outcome(task, form, "", tolerant=False) == "tests fail"
    assert outcome(task, form, reference_edit(task, form), tolerant=False) == "pass"
