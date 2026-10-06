"""Weighting the families and drawing each program's mutants (specs/seeded-failures/ R2.5, R2.6)."""

from collections import Counter

from lotml_harness.guide import seeded
from lotml_harness.guide.seeded import FAMILIES, apportion, draw, shares, weights

REPORT = """\
| model | language | pass | does not check | tests fail | does not run | first errors |
|---|---|---:|---:|---:|---:|---|
| a | lotml | 10 | 5 | 3 | 0 | E0201 2, E0302 1, E0003 2 |
| a | python | 12 | 0 | 6 | 0 | — |
| b | lotml | 9 | 4 | 2 | 1 | E0204 3, E0203 1 |
"""


def test_weights_count_first_errors_by_family_and_failed_tests_as_meaning():
    found = weights(REPORT)
    assert found.families == {
        "names": 2,
        "types": 3,
        "calls": 1,
        "mutability": 1,
        "meaning": 5,
    }
    assert found.unmapped == {"E0003": 2}, "a syntax error is no operator's"
    assert found.sources == ["harness/results/phase1.md"]


def test_the_trace_dataset_s_repair_codes_add_to_the_weights():
    found = weights(REPORT, Counter({"E0201": 4, "E0999": 1}))
    assert found.families["names"] == 6
    assert found.unmapped["E0999"] == 1
    assert len(found.sources) == 2


def test_the_committed_report_weights_every_family():
    found = weights(seeded.PHASE1.read_text(encoding="utf-8"))
    assert all(found.families[f] > 0 for f in FAMILIES)


def test_apportion_splits_by_largest_remainder():
    assert apportion(10, {"names": 1, "types": 1, "calls": 1}) == {
        "names": 4,
        "types": 3,
        "calls": 3,
    }
    split = apportion(60, {"names": 66, "types": 32, "calls": 4, "mutability": 43, "meaning": 50})
    assert sum(split.values()) == 60
    assert split["names"] > split["meaning"] > split["mutability"] > split["types"] > split["calls"]
    assert apportion(5, {"names": 0}) == {"names": 0}


def test_a_family_short_of_mutants_passes_its_share_on_by_weight():
    weight = {"names": 2, "types": 1, "calls": 1, "mutability": 0, "meaning": 0}
    given = shares(8, weight, {"names": 1, "types": 10, "calls": 10, "mutability": 9})
    assert given == {"names": 1, "types": 4, "calls": 3, "mutability": 0, "meaning": 0}
    assert sum(shares(60, weight, {"names": 2, "types": 3, "calls": 1}).values()) == 6


def program(task: str, problem: str) -> seeded.Program:
    return seeded.Program(task, problem, "train", "bench", "", {}, ("a.lotml",), "")


def listed(count: int, family: str = "names", tag: str = "") -> list[dict]:
    return [{"family": family, "text": f"{family}{tag}{i}"} for i in range(count)]


def test_draws_are_uniform_within_a_family_and_reproducible():
    weight = dict.fromkeys(FAMILIES, 1)
    pool = listed(100) + listed(100, "meaning")
    first = draw(program("t", "humaneval/0"), pool, weight, set())
    assert len(first) == 60
    assert Counter(m["family"] for m in first) == {"names": 30, "meaning": 30}
    assert first == draw(program("t", "humaneval/0"), pool, weight, set())
    other = draw(program("u", "humaneval/0"), pool, weight, set())
    assert first != other, "the generator is seeded with the program"


def test_an_identical_mutant_is_drawn_once_per_problem():
    weight = dict.fromkeys(FAMILIES, 1)
    seen: set[str] = set()
    first = draw(program("t", "humaneval/0"), listed(3), weight, seen)
    again = draw(program("u", "humaneval/0"), listed(3) + listed(2, tag="x"), weight, seen)
    assert len(first) == 3
    assert {m["text"] for m in again} == {"namesx0", "namesx1"}
    elsewhere = draw(program("v", "humaneval/1"), listed(3), weight, seen)
    assert len(elsewhere) == 3, "another problem may draw the same text"
