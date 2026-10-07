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
    records = [record(i) for i in range(10)]
    found = grpo.rows(records, [7, 2, 5, 9, 1], 4, seed=0)
    assert len(found) == 4
    assert [m["role"] for m in found[0]["prompt"]] == ["system", "user"]
    assert json.loads(found[0]["state"]) == STATE
    numbers = [int(r["prompt"][1]["content"].removeprefix("u")) for r in found]
    assert numbers == sorted(numbers) and set(numbers) <= {7, 2, 5, 9, 1}
    assert json.loads(found[0]["truth"]) == [f"f{numbers[0]}"]
    assert grpo.rows(records, [7, 2, 5, 9, 1], 4, seed=0) == found, "the same draw for the seed"
    assert len(grpo.rows(records, [3], 4, seed=0)) == 1


def test_the_pool_keeps_what_the_guide_sometimes_solves_and_a_share_of_the_rest():
    means = [0.0, 0.5, 1.0, 0.25, 1.0, 1.0, 0.0, 0.75, 1.0, 1.0]
    samples = [{"index": i, "mean": m} for i, m in enumerate(means)]
    found = grpo.pool(samples, easy=0.34, seed=0)
    sometimes = {1, 3, 7}
    assert sometimes <= set(found)
    assert not {0, 6} & set(found), "the never-solved give nothing to learn"
    assert len(set(found) - sometimes) == 1, "a third of three, from the always-solved"
    assert found == grpo.pool(samples, easy=0.34, seed=0)
    assert grpo.pool(samples, easy=0.0, seed=0) == [1, 3, 7]


def test_the_random_reward_ignores_the_answers_and_holds_even_odds():
    draw = grpo.RandomReward(seed=0)
    found = draw(["a"] * 2000)
    assert set(found) == {0.0, 1.0}
    assert 0.45 < sum(found) / len(found) < 0.55
    assert grpo.RandomReward(seed=0)(["x"] * 10) == grpo.RandomReward(seed=0)(["y"] * 10)
    assert draw.__name__ == "random"


def test_the_summary_reads_reward_alike_entropy_clipping_and_evaluations():
    history = [
        {
            "reward": 0.4,
            "frac_reward_zero_std": 0.5,
            "entropy": 0.9,
            "completions/clipped_ratio": 0.0,
        },
        {"eval_reward": 0.5},
        {
            "reward": 0.6,
            "frac_reward_zero_std": 0.3,
            "entropy": 0.7,
            "completions/clipped_ratio": 0.1,
        },
        {"eval_reward": 0.55},
    ]
    found = grpo.summary(history)
    assert (found["reward_first"], found["reward_last"]) == (0.4, 0.6)
    assert found["alike"] == 0.4 and found["clipped"] == 0.05
    assert (found["entropy_first"], found["entropy_last"]) == (0.9, 0.7)
    assert found["eval_rewards"] == [0.5, 0.55]
    assert grpo.summary([])["reward_first"] is None


def test_the_settings_are_adr_0020_s():
    settings = grpo.Settings()
    assert (settings.batch * settings.accumulate) % settings.generations == 0
    assert (settings.loss, settings.scale, settings.beta) == ("dr_grpo", "none", 0.0)
    assert (settings.epsilon, settings.epsilon_high, settings.temperature) == (0.2, 0.28, 1.0)
    assert settings.learning_rate == 1e-5 and settings.completion == 1024
