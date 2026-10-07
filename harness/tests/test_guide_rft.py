"""Rejection-sampled fine-tuning's examples (specs/training-pipeline/ R3.7)."""

import json

from lotml_harness.guide import rft


def record(i: int) -> dict:
    return {
        "messages": [
            {"role": "system", "content": "s"},
            {"role": "user", "content": f"u{i}"},
            {"role": "assistant", "content": f"target{i}"},
        ]
    }


def answer(text: str, first: bool, edit: str) -> dict:
    return {"text": text, "first": first, "edit": edit}


def test_passing_answers_are_kept_distinct_and_capped_and_a_target_stands_in_for_none():
    records = [record(0), record(1), record(2)]
    rows = [
        {"index": 0, "answers": [answer("a", True, "passes"), answer("a", True, "passes"),
                                 answer("b", True, "passes"), answer("c", True, "passes")]},
        {"index": 1, "answers": [answer("x", True, "edit-fails-check"),
                                 answer("y", False, "passes")]},
        {"index": 2, "answers": [answer("z", True, "passes")]},
    ]  # fmt: skip
    found, counts = rft.examples(records, rows, keep=2)
    completions = [e["completion"][0]["content"] for e in found]
    assert completions == ["a", "b", "target1", "z"]
    assert counts == {"answers": 3, "targets": 1, "records": 3}
    assert [m["role"] for m in found[0]["prompt"]] == ["system", "user"]
    assert json.dumps(found)
