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
    assert settings.revision.startswith("ea3f2471") and len(settings.revision) == 40


def test_llama_cpp_s_tools_are_found_by_exact_name_and_only_once(tmp_path: Path):
    (tmp_path / "bin" / "llama-quantize-docs").mkdir(parents=True)
    (tmp_path / "bin" / "llama-quantize.exe").write_bytes(b"")
    assert train._tool(tmp_path, ("llama-quantize", "llama-quantize.exe")).name == (
        "llama-quantize.exe"
    )
    (tmp_path / "other").mkdir()
    (tmp_path / "other" / "llama-quantize").write_bytes(b"")
    with pytest.raises(FileNotFoundError):
        train._tool(tmp_path, ("llama-quantize", "llama-quantize.exe"))
    with pytest.raises(FileNotFoundError):
        train._tool(tmp_path, ("convert_hf_to_gguf.py",))
