"""Judging mutants and writing the kept ones (specs/seeded-failures/ R2.3, R2.4, R2.7, R3.1,
R3.3)."""

import json
from collections import Counter
from pathlib import Path

import pytest

from lotml_harness.guide import seeded
from lotml_harness.guide.seeded import HeldOut, Program, Verdict, judge

CODE = """\
fn add(a: int, b: int) -> int:
    var total = a
    total += b
    return total

fn spin(n: int) -> int:
    var k = 0
    while k < n:
        k += 1
    return k
"""
BLOCKS = """
test "hidden: 1":
    assert add(1, 2) == 3

test "hidden: 2":
    assert spin(3) == 3
"""


def program(problem: str = "humaneval/0", bucket: str = "train") -> Program:
    files = {"solution.lot": CODE + BLOCKS}
    return Program(
        "humaneval-0", problem, bucket, "humaneval-original", "Write it.", files,
        ("solution.lot",), "the notice",
    )  # fmt: skip


def mutated(old: str, new: str) -> str:
    text = CODE + BLOCKS
    assert old in text
    return text.replace(old, new, 1)


def test_a_mutant_check_refuses_is_kept_with_its_diagnostics():
    verdict = judge(program(), "solution.lot", mutated("return total", "return totals"))
    assert (verdict.kept, verdict.reason) == (True, "check")
    assert [d["code"] for d in verdict.diagnostics] == ["E0201"]


def test_a_mutant_check_fix_makes_clean_is_dropped():
    verdict = judge(program(), "solution.lot", mutated("var total = a", "total = a"))
    assert (verdict.kept, verdict.reason) == (False, "fixable")


def test_a_mutant_that_checks_and_fails_a_test_is_kept_with_the_block_s_values():
    verdict = judge(program(), "solution.lot", mutated("total += b", "total -= b"))
    assert (verdict.kept, verdict.reason) == (True, "test")
    assert verdict.failing["name"] == "hidden: 1"
    assert (verdict.failing["left"], verdict.failing["right"]) == ("-1", "3")


def test_an_equivalent_mutant_and_a_hang_are_dropped():
    same = judge(program(), "solution.lot", mutated("var total = a", "var total = a + 0"))
    assert (same.kept, same.reason) == (False, "equivalent")
    hang = judge(program(), "solution.lot", mutated("k += 1", "pass"), deadline=3)
    assert (hang.kept, hang.reason) == (False, "timeout")


def test_mutants_are_listed_from_the_graded_files_outside_the_test_blocks():
    found, failed = seeded.listed(program())
    assert found
    assert failed == {}
    assert {m["file"] for m in found} == {"solution.lot"}
    assert all(m["start"] < len(CODE) for m in found), "no mutant falls in a test block"


def test_seeding_writes_each_kept_mutant_as_a_repair_marked_seeded(monkeypatch):
    found = [
        {"family": "names", "operator": "misspell-name", "declaration": "add",
         "file": "solution.lot", "text": mutated("return total", "return totals")},
        {"family": "meaning", "operator": "swap-arithmetic", "declaration": "add",
         "file": "solution.lot", "text": mutated("var total = a", "var total = a + 0")},
    ]  # fmt: skip
    monkeypatch.setattr(seeded, "listed", lambda program: (found, Counter()))
    weighting = seeded.Weights(Counter(dict.fromkeys(seeded.FAMILIES, 1)), Counter(), ["x"])
    records, tally = seeded.seed([program()], weighting, "lotml 0.1.0", workers=2)
    [record] = records
    assert record["before"] == found[0]["text"]
    assert record["after"] == program().files["solution.lot"]
    assert record["diagnostics"][0]["code"] == "E0201"
    assert record["changed"] == [4]
    assert record["prompt"] == "Write it."
    assert record["meta"] | {} == {
        "task": "humaneval-0",
        "source": "humaneval-original",
        "model": None,
        "arm": None,
        "compiler": "lotml 0.1.0",
        "problem": "humaneval/0",
        "split": "train",
        "origin": "seeded",
        "operator": "misspell-name",
        "family": "names",
        "declaration": "add",
    }
    assert tally.outcomes[("names", "misspell-name")] == {"check": 1}
    assert tally.outcomes[("meaning", "swap-arithmetic")] == {"equivalent": 1}
    assert tally.programs == {"humaneval-original": 1}


def test_records_are_written_by_split_with_their_notices(tmp_path: Path):
    record = {"meta": {"split": "validation", "source": "humaneval-original", "problem": "p"}}
    out = seeded.write([record], [program()], "2026-10-06", tmp_path)
    assert (out / "train" / "repairs.jsonl").read_text(encoding="utf-8") == ""
    lines = (out / "validation" / "repairs.jsonl").read_text(encoding="utf-8").splitlines()
    assert [json.loads(line) for line in lines] == [record]
    assert "the notice" in (out / "NOTICE").read_text(encoding="utf-8")
    held = {"meta": {"split": "held-out", "source": "x", "problem": "humaneval/4"}}
    with pytest.raises(HeldOut):
        seeded.write([held], [], "2026-10-06", tmp_path)


def test_the_report_counts_programs_weights_and_every_operator_s_outcomes():
    tally = seeded.Tally()
    tally.programs["bench"] = 8
    tally.left_out["humaneval-original"] = Counter({"rules": 22})
    tally.splits["train"] = 5
    tally.listed[("names", "misspell-name")] = 40
    tally.drawn[("names", "misspell-name")] = 20
    tally.outcomes[("names", "misspell-name")].update({"check": 18, "fixable": 2})
    weighting = seeded.weights(seeded.PHASE1.read_text(encoding="utf-8"))
    text = seeded.markdown(tally, weighting, "lotml 0.1.0", "2026-10-06")
    assert "| bench | 8 | — |" in text
    assert "| humaneval-original | 0 | rules 22 |" in text
    assert "| names | misspell-name | 40 | 20 | 18 | 0 | 0 | 2 | 0 | 0 |" in text
    assert "| names | 66 | 20 |" in text
    assert "E0003 92" in text
    assert "Kept 18 of 20 drawn" in text


def test_a_verdict_is_a_plain_record():
    assert Verdict(True, "check").diagnostics is None


def test_a_file_mutate_cannot_list_and_files_the_safe_layer_refuses_are_counted():
    missing = Program("t", "humaneval/0", "train", "x", "", {"a.lotml": CODE}, ("b.lotml",), "")
    assert seeded.listed(missing) == ([], {"mutate failed": 1})
    unsafe = Program("t", "humaneval/0", "train", "x", "", {"a.txt": CODE}, ("a.txt",), "")
    assert seeded.listed(unsafe) == ([], {"unsafe files": 1})
    assert judge(unsafe, "a.txt", CODE).reason == "refused"


def test_the_report_names_files_whose_mutants_could_not_be_listed():
    tally = seeded.Tally()
    tally.unlisted["mutate failed"] = 2
    weighting = seeded.weights(seeded.PHASE1.read_text(encoding="utf-8"))
    text = seeded.markdown(tally, weighting, "lotml 0.1.0", "2026-10-06")
    assert "could not be listed: mutate failed 2." in text
