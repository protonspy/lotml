"""The reinforcement learning's dataset and settings (specs/training-pipeline/ R3.3)."""

import json

from lotml_harness.guide import grpo

STATE = {
    "task": None,
    "path": "solution.lotml",
    "text": "fn f() -> int:\n",
    "diagnostics": [],
    "failing": None,
}


def record(i: int) -> dict:
    answer = {
        "locations": [{"path": "solution.lotml", "symbol": f"f{i}", "lines": [1, 2]}],
        "kind": "body",
        "edit": None,
    }
    return {
        "messages": [
            {"role": "system", "content": "s"},
            {"role": "user", "content": f"u{i}"},
            {"role": "assistant", "content": json.dumps(answer)},
        ],
        "state": STATE,
        "meta": {"problem": "humaneval/0", "split": "train"},
    }


def test_rows_hold_the_prompt_without_the_answer_and_the_reward_s_columns_as_json():
    found = grpo.rows([record(i) for i in range(10)], 4, seed=0)
    assert len(found) == 4
    assert [m["role"] for m in found[0]["prompt"]] == ["system", "user"]
    assert json.loads(found[0]["state"]) == STATE
    number = found[0]["prompt"][1]["content"].removeprefix("u")
    assert json.loads(found[0]["truth"]) == [f"f{number}"]
    assert grpo.rows([record(i) for i in range(10)], 4, seed=0) == found, (
        "the same draw for the seed"
    )
    assert len(grpo.rows([record(0)], 4, seed=0)) == 1


def test_a_step_takes_whole_groups():
    settings = grpo.Settings()
    assert (settings.batch * settings.accumulate) % settings.generations == 0
    assert settings.completion == 1024
