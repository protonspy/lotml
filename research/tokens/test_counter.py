import pytest
from counter import TOKENIZERS, count, load


@pytest.fixture(scope="module", params=sorted(TOKENIZERS))
def name(request):
    load(request.param)
    return request.param


def test_empty_text_costs_zero_tokens(name):
    assert count(name, "") == 0


def test_single_common_word_is_one_token(name):
    assert count(name, "return") == 1


def test_count_grows_with_text(name):
    assert count(name, "return x") < count(name, "return x + y * z")


def test_o200k_reference_value():
    assert count("o200k", "hello world") == 2
