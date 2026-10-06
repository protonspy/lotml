"""The problem split: one answer per original problem for every source derived from it
(specs/trace-dataset/ R4.1-R4.4)."""

from collections import Counter
from hashlib import sha256

import pytest

from lotml_harness import split


@pytest.mark.parametrize(
    ("ident", "problem"),
    [
        ("humaneval/12", "humaneval/12"),
        ("humaneval-12", "humaneval/12"),
        ("HumanEval_12_longest", "humaneval/12"),
        ("mbpp/101", "mbpp/101"),
        ("mbpp-101", "mbpp/101"),
        ("mbpp_101_kth_element", "mbpp/101"),
        ("stock-take", "bench/stock-take"),
    ],
)
def test_every_derived_id_names_its_original_problem(ident, problem):
    assert split.problem(ident) == problem


@pytest.mark.parametrize(
    "ident",
    ["livecodebench/3", "HumanEval/12", "humaneval-", "humaneval-1x", "no-such-task", "../x", ""],
)
def test_an_id_of_no_known_form_is_refused(ident):
    with pytest.raises(ValueError, match="no known problem"):
        split.problem(ident)


def bucket(problem: str) -> str:
    """R4.2 restated: the first eight bytes of SHA-256(salt + id) over 2^64, cut at 60 and 75."""
    share = int.from_bytes(sha256(("lotml-split-1" + problem).encode()).digest()[:8]) / 2**64
    return "train" if share < 0.60 else "validation" if share < 0.75 else "held-out"


def test_humaneval_is_split_by_the_salted_hash_near_sixty_fifteen_twenty_five():
    problems = [f"humaneval/{n}" for n in range(164)]
    assert [split.split(p) for p in problems] == [bucket(p) for p in problems]
    counts = Counter(split.split(p) for p in problems)
    assert abs(counts["train"] / 164 - 0.60) < 0.08
    assert abs(counts["validation"] / 164 - 0.15) < 0.08
    assert abs(counts["held-out"] / 164 - 0.25) < 0.08


def test_mbpp_is_held_out_whole_and_the_benchmark_is_train():
    assert {split.split(f"mbpp/{n}") for n in range(1, 975)} == {"held-out"}
    assert split.split("bench/stock-take") == "train"


def test_a_derived_id_lands_with_its_original():
    for n in range(164):
        assert split.split(split.problem(f"humaneval-{n}")) == split.split(f"humaneval/{n}")
        assert split.split(split.problem(f"HumanEval_{n}_f")) == split.split(f"humaneval/{n}")


def test_split_refuses_what_is_not_a_problem_id():
    with pytest.raises(ValueError, match="no known problem"):
        split.split("humaneval-3")
