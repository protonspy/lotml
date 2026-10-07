"""Seeded failures that join two mutants of one program in different declarations
(specs/seeded-failures/ R2.8)."""

from lotml_harness.guide import seeded

ORIGINAL = (
    'fn média(a: int) -> int:\n    """Soma."""\n    return a + 1\n\n'
    "fn dobro(x: int) -> int:\n    return x * 2\n"
)


def mutant(start_text: str, old: str, new: str, declaration: str, operator: str = "op") -> dict:
    data = ORIGINAL.encode("utf-8")
    start = data.index(start_text.encode("utf-8")) + start_text.encode("utf-8").index(
        old.encode("utf-8")
    )
    end = start + len(old.encode("utf-8"))
    return {"start": start, "end": end, "replacement": new, "declaration": declaration,
            "operator": operator, "family": "types", "file": "a.lot"}  # fmt: skip


def test_two_mutants_are_joined_by_their_byte_spans_and_overlapping_ones_are_not():
    first = mutant("return a + 1", "+", "-", "média")
    second = mutant("return x * 2", "2", "3", "dobro")
    expected = ORIGINAL.replace("a + 1", "a - 1").replace("x * 2", "x * 3")
    assert seeded.joined(ORIGINAL, first, second) == expected
    assert seeded.joined(ORIGINAL, second, first) == expected
    same = mutant("return a + 1", "a + 1", "a", "média")
    assert seeded.joined(ORIGINAL, first, same) is None


def test_pairs_join_mutants_of_one_file_in_different_declarations_up_to_a_count():
    program = seeded.Program(
        task="t", source="bench", problem="bench/t", split="train", prompt="p",
        files={"a.lot": ORIGINAL}, graded=("a.lot",), notice="",
    )  # fmt: skip
    drawn = [
        mutant("return a + 1", "+", "-", "média", "flip"),
        mutant("return a + 1", "1", "2", "média", "bump"),
        mutant("return x * 2", "2", "3", "dobro", "bump"),
    ]
    found = seeded.pairs(program, drawn, 5)
    assert len(found) == 2, "only pairs across the two declarations"
    assert {p["declaration"] for p in found} == {"média, dobro"}
    assert all(p["family"] == "pair" and "+" in p["operator"] for p in found)
    assert found == seeded.pairs(program, drawn, 5), "the same draw for the program"
    assert len(seeded.pairs(program, drawn, 1)) == 1
