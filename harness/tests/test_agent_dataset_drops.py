"""Records left out of the dataset: a secret, or a hidden test's assert the prompt does not show
(specs/trace-dataset/ R3.7)."""

import pytest

from lotml_harness.agent.dataset import dropped

HIDDEN = {"assert add(1, 2) == 3", "assert add(-1, 1) == 0"}


def record(text: str, prompt: str = "Write add. >>> add(-1, 1) == 0") -> dict:
    return {"prompt": prompt, "before": text, "after": text, "meta": {"task": "t"}}


def test_a_record_holding_a_secret_is_dropped(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setenv("OPENROUTER_API_KEY", "sk-or-v1-abcdefghijklmnopqrstuvwx")
    assert dropped(record("key = sk-or-v1-abcdefghijklmnopqrstuvwx"), HIDDEN) == "a secret"


def test_a_hidden_assert_the_prompt_does_not_show_drops_the_record():
    leaked = record('test "t":\n    assert   add(1, 2)  ==  3\n')
    assert dropped(leaked, HIDDEN) == "a hidden test"


def test_a_hidden_assert_followed_by_a_comment_still_drops_the_record():
    leaked = record('test "t":\n    assert add(1, 2) == 3  # from the task\n')
    assert dropped(leaked, HIDDEN) == "a hidden test"


def test_an_assert_the_prompt_also_shows_is_kept():
    copied = record('test "t":\n    assert add(-1, 1) == 0\n')
    assert dropped(copied, HIDDEN) is None


def test_a_docstring_example_shows_the_assert():
    prompt = "Write add.\n    >>> add(1, 2)\n    3\n"
    copied = record('test "t":\n    assert add(1, 2) == 3\n', prompt)
    assert dropped(copied, HIDDEN) is None


def test_a_call_and_a_value_apart_in_the_prompt_do_not_show_the_assert():
    prompt = "Write add, which takes 3 numbers' worth of care. >>> add(1, 2) is commutative"
    leaked = record('test "t":\n    assert add(1, 2) == 3\n', prompt)
    assert dropped(leaked, HIDDEN) == "a hidden test"


def test_a_longer_value_in_the_prompt_does_not_show_a_shorter_one():
    leaked = record('test "t":\n    assert add(1, 2) == 3\n', "Write add. >>> add(1, 2) == 30")
    assert dropped(leaked, HIDDEN) == "a hidden test"


def test_an_assert_inside_a_longer_one_is_not_found():
    kept = record('test "t":\n    assert add(1, 2) == 30\n', "Write add.")
    assert dropped(kept, HIDDEN) is None


def test_a_clean_record_is_kept():
    assert dropped(record("fn add(a: int, b: int) -> int:\n    return a + b\n"), HIDDEN) is None
