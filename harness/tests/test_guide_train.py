"""The guide's training data, as the trainer reads it (plans/harness-guide.md 2.2)."""

import json
from pathlib import Path

import pytest

from lotml_harness.guide import train
from lotml_harness.guide.train import HeldOut, examples, load


def record(problem: str) -> dict:
    return {
        "messages": [
            {"role": "system", "content": "s"},
            {"role": "user", "content": "u"},
            {"role": "assistant", "content": "{}"},
        ],
        "meta": {"problem": problem},
    }


def test_a_record_becomes_a_prompt_and_the_answer_its_completion():
    [example] = examples([record("humaneval/0")])
    assert [m["role"] for m in example["prompt"]] == ["system", "user"]
    assert example["completion"] == [{"role": "assistant", "content": "{}"}]


def write(directory: Path, bucket: str, *problems: str) -> Path:
    directory.mkdir(parents=True, exist_ok=True)
    lines = "".join(json.dumps(record(p)) + "\n" for p in problems)
    (directory / f"{bucket}.jsonl").write_text(lines, encoding="utf-8")
    return directory


def test_a_split_is_loaded_with_every_problem_checked_against_it(tmp_path: Path):
    assert (
        len(load(write(tmp_path / "ok", "train", "humaneval/0", "bench/stock-take"), "train")) == 2
    )
    with pytest.raises(HeldOut):
        load(write(tmp_path / "held", "train", "humaneval/4"), "train")
    with pytest.raises(HeldOut):
        load(write(tmp_path / "other", "train", "humaneval/0", "mbpp/2"), "train")


def test_the_settings_are_the_pilot_s():
    settings = train.Settings()
    assert (settings.rank, settings.alpha, settings.learning_rate) == (16, 32, 2e-4)
    assert (settings.batch, settings.accumulate, settings.memory_fraction) == (1, 16, 0.85)
    assert settings.model == "Qwen/Qwen2.5-Coder-0.5B-Instruct"
