from pathlib import Path

import pytest
from check import leaks, parse_error

CORPUS = Path(__file__).parent.parent / "tokens" / "corpus"


@pytest.mark.parametrize("variant", ["a", "b"])
def test_every_corpus_program_parses_in_its_variant(variant):
    for path in sorted(CORPUS.glob(f"*/{variant}.x")):
        assert parse_error(variant, path.read_text(encoding="utf-8")) is None, path


def test_variant_a_rejects_case_arms_and_b_accepts_them():
    src = "fn f(s: Shape) -> int:\n    match s:\n        case Circle(r):\n            return 1\n"
    assert parse_error("a", src) is not None
    assert parse_error("b", src) is None


def test_variant_b_rejects_bare_arms():
    src = "fn f(s: Shape) -> int:\n    match s:\n        Circle(r):\n            return 1\n"
    assert parse_error("b", src) is not None


def test_python_function_parses_in_neither_variant():
    src = "def f(x: int) -> int:\n    return x\n"
    assert parse_error("a", src) is not None
    assert parse_error("b", src) is not None


def test_python_constructs_leak_in_both_variants():
    src = "def f(x):\n    try:\n        raise E()\n    except E:\n        pass\n"
    for variant in "ab":
        assert {"def", "try", "raise", "except"} <= set(leaks(variant, src))


def test_none_and_lambda_leak_only_in_variant_a():
    src = "fn f() -> int?:\n    g = lambda x: x\n    return None\n"
    assert {"None", "lambda"} <= set(leaks("a", src))
    assert leaks("b", src) == []


def test_strings_and_comments_do_not_leak():
    src = 'fn f() -> str:\n    return "def raise None"  # class try\n'
    assert leaks("a", src) == []


@pytest.mark.parametrize("variant", ["a", "b"])
def test_power_operator_parses(variant):
    assert (
        parse_error(variant, "fn f(x: f64) -> f64:\n    return (x * x) ** 0.5\n")
        is None
    )


def test_negative_literal_pattern_parses():
    src = "fn f(r: Res) -> int:\n    match r:\n        case Err(Neg(-5)):\n            return 1\n"
    assert parse_error("b", src) is None


def test_hostile_input_is_checked_in_linear_time():
    import time

    hostile = ['"\\' * 4000, "\n" * 40000]
    start = time.perf_counter()
    for text in hostile:
        leaks("a", text)
        parse_error("a", text)
    assert time.perf_counter() - start < 2.0
