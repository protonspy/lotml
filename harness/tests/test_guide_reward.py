"""The reward of the guide's reinforcement learning: an answer judged by the compiler
(specs/training-pipeline/ R4.1-R4.4)."""

import json

import pytest

from lotml_harness.guide import reward
from lotml_harness.guide.reward import Judged, Reward, judge, score

BROKEN = "fn count() -> int:\n    n = 0\n    n += 1\n    return n\n"
STATE = {"task": None, "path": "solution.lotml", "text": BROKEN, "diagnostics": [], "failing": None}
WRONG = (
    'fn add(a: int, b: int) -> int:\n    return a - b\n\ntest "adds":\n    assert add(1, 2) == 3\n'
)
FAILING = {
    "task": None,
    "path": "solution.lotml",
    "text": WRONG,
    "diagnostics": [],
    "failing": {"name": "adds", "line": 5, "left": "-1", "right": "3"},
}


def answer(symbols: list[str | None], body: str | None, symbol: str = "count") -> str:
    edit = None
    if body is not None:
        edit = {
            "tool": "replace",
            "arguments": {"path": "solution.lotml", "symbol": symbol, "part": "body", "text": body},
        }
    locations = [{"path": "solution.lotml", "symbol": s, "lines": [1, 4]} for s in symbols]
    return json.dumps({"locations": locations, "kind": "body", "edit": edit})


@pytest.mark.parametrize(
    ("judged", "truth", "expected"),
    [
        (None, ["count"], 0.0),
        (Judged(False, [], "none"), ["count"], 0.0),
        (Judged(True, ["count"], "passes"), ["count"], 1.0),
        (Judged(True, ["count"], "edit-fails-check"), ["count"], 0.5),
        (Judged(True, ["other", "count"], "passes"), ["count"], (10 / 11 + 1) / 2),
        (Judged(True, ["other", "count"], "none"), ["count"], 10 / 11 / 2),
        (Judged(True, ["other"], "passes"), ["count"], 0.5),
        (Judged(True, ["other"], "none"), ["count"], 0.0),
        (Judged(True, [None], "none"), [None], 0.5),
        (Judged(True, ["count"], "none"), ["count", "helper"], 10 / 19 / 2),
        (Judged(True, ["count", "count"], "none"), ["count"], 0.5),
        (Judged(True, ["count", "ghost"], "passes", ("ghost",)), ["count"], 0.5),
    ],
)
def test_the_locations_score_an_f_score_with_beta_3_averaged_with_the_edit(judged, truth, expected):
    assert score(judged, truth) == pytest.approx(expected)


def test_naming_more_declarations_never_scores_more_than_naming_the_right_one():
    alone = score(Judged(True, ["count"], "none"), ["count"])
    padded = score(Judged(True, ["a", "b", "count"], "none"), ["count"])
    assert padded < alone


def test_the_judge_names_the_declarations_the_file_does_not_have():
    found = judge(answer(["count", "ghost"], None), STATE)
    assert (found.valid, found.unknown) == (True, ("ghost",))
    assert judge(answer(["count"], None), STATE).unknown == ()


def test_the_compiler_judges_an_edit_that_fixes_the_file_and_one_that_copies_it():
    fixed = judge(answer(["count"], "    var n = 0\n    n += 1\n    return n"), STATE)
    assert fixed == Judged(True, ["count"], "passes")
    copied = judge(answer(["count"], "    n = 0\n    n += 1\n    return n"), STATE)
    assert copied == Judged(True, ["count"], "edit-fails-check")
    assert judge("not json", STATE) == Judged(False, [], "none")


def test_a_failing_block_is_run_with_the_edit_made():
    passing = judge(answer(["add"], "    return a + b", "add"), FAILING)
    assert passing == Judged(True, ["add"], "passes")
    assert judge(answer(["add"], "    return a * b", "add"), FAILING).edit == "edit-fails-test"


def test_a_judgment_past_its_deadline_is_none(monkeypatch: pytest.MonkeyPatch):
    monkeypatch.setattr(reward.safe, "lotml", lambda *_, **__: None)
    assert judge(answer(["count"], None), STATE) is None


def test_the_reward_scores_a_group_of_completions_and_counts_the_unjudged(
    monkeypatch: pytest.MonkeyPatch,
):
    verdicts = iter([Judged(True, ["count"], "passes"), None, Judged(True, ["x"], "none")])
    monkeypatch.setattr(reward, "judge", lambda text, state, deadline=0: next(verdicts))
    scorer = Reward(workers=1)
    completions = [[{"role": "assistant", "content": "a"}] for _ in range(3)]
    found = scorer(completions, state=[STATE] * 3, truth=[["count"]] * 3)
    assert found == [1.0, 0.0, 0.0]
    assert scorer.unjudged == 1
    assert scorer.__name__ == "compiler"


def test_the_reward_reads_the_dataset_s_json_columns(monkeypatch: pytest.MonkeyPatch):
    seen = []
    monkeypatch.setattr(
        reward,
        "judge",
        lambda text, state, deadline=0: seen.append(state) or Judged(True, [None], "none"),
    )
    found = Reward(workers=1)(["x"], state=[json.dumps(STATE)], truth=[json.dumps([None])])
    assert found == [0.5]
    assert seen == [STATE]
