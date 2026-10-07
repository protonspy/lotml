"""Several answers to each record from the guide, sampled at temperature 1.0 as group-relative
policy optimization samples them, each judged by the compiler (specs/training-pipeline/ R3.6).

What the sampling feeds: rejection-sampled fine-tuning keeps the answers that pass (R3.7), the
reinforcement-learning pool keeps the records the guide sometimes solves (R3.8), and the report's
pass@k counts the records with an answer right among the first k (R5.1).

The heavy libraries are imported where they are used, so the judging and counting are tested
without them.
"""

import json
import math
import random
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from pathlib import Path

from lotml_harness.guide import reward


@dataclass(frozen=True)
class Settings:
    """How answers are sampled; the report keeps them beside the counts."""

    answers: int = 4
    temperature: float = 1.0
    completion: int = 1024
    """The guide tool's answer budget."""
    prompts: int = 16
    """Records whose answers are generated in one batch."""
    records: int | None = None
    """Records drawn by `seed`, or all of them."""
    seed: int = 0
    workers: int = 8


def truth(record: dict) -> list[str | None]:
    """The symbols of the declarations the record's real fix changed."""
    return [loc["symbol"] for loc in json.loads(record["messages"][-1]["content"])["locations"]]


def drawn(records: list[dict], count: int | None, seed: int) -> list[dict]:
    """`count` records drawn by `seed`, in their order, or all of them."""
    if count is None or count >= len(records):
        return list(records)
    keep = set(random.Random(seed).sample(range(len(records)), count))  # noqa: S311 - a reproducible draw
    return [r for i, r in enumerate(records) if i in keep]


def judged(records: list[dict], answers: list[list[str]], workers: int) -> list[dict]:
    """Each record's answers judged by the compiler and scored: per record its problem, its truth,
    every answer with its verdict and reward, and their mean."""
    jobs = [(i, text) for i, texts in enumerate(answers) for text in texts]
    with ThreadPoolExecutor(max_workers=workers) as pool:
        verdicts = list(pool.map(lambda job: reward.judge(job[1], records[job[0]]["state"]), jobs))
    found = [{"problem": r["meta"]["problem"], "truth": truth(r), "answers": []} for r in records]
    for (i, text), verdict in zip(jobs, verdicts, strict=True):
        wanted = found[i]["truth"]
        found[i]["answers"].append(
            {
                "text": text,
                "valid": verdict is not None and verdict.valid,
                "judged": verdict is not None,
                "edit": None if verdict is None else verdict.edit,
                "first": verdict is not None
                and bool(verdict.symbols)
                and verdict.symbols[0] in wanted,
                "reward": reward.score(verdict, wanted),
            }
        )
    for row in found:
        rewards = [a["reward"] for a in row["answers"]]
        row["mean"] = sum(rewards) / len(rewards) if rewards else 0.0
    return found


def passed(answer: dict) -> bool:
    """An answer whose first location names a declaration the fix changed and whose edit passes."""
    return answer["first"] and answer["edit"] == reward.PASSES


def pass_at(n: int, c: int, k: int) -> float:
    """The unbiased pass@k from `n` answers of which `c` are right: the chance that `k` of them,
    drawn without replacement, hold at least one."""
    if n - c < k:
        return 1.0
    return 1.0 - math.comb(n - c, k) / math.comb(n, k)


def index(value: object, count: int) -> int:
    """`value`, a sampled row's index into its `count` records; ValueError when it is not one —
    a row read back from the repository is not trusted to point where it should."""
    if type(value) is not int or not 0 <= value < count:
        raise ValueError(f"{value!r} is not an index into {count} records")
    return value


def pass_rates(rows: list[dict], ks: tuple[int, ...] = (1, 4, 8)) -> dict:
    """pass@k of the first location and of the whole answer — location and edit — over the rows,
    for every k no larger than the answers each row holds."""
    found: dict = {}
    for k in ks:
        usable = [r for r in rows if len(r["answers"]) >= k]
        if not usable:
            continue
        for name, right in (("location", lambda a: a["first"]), ("answer", passed)):
            rates = [pass_at(len(r["answers"]), sum(map(right, r["answers"])), k) for r in usable]
            found[f"{name}@{k}"] = sum(rates) / len(rates)
    return found


def generate(
    model: object, tokenizer: object, records: list[dict], settings: Settings
) -> list[list[str]]:
    """`settings.answers` answers to each record's prompt — its system and user messages — from a
    loaded model, sampled at `settings.temperature`, `settings.prompts` records at a time."""
    import torch

    tokenizer.padding_side = "left"
    found: list[list[str]] = []
    for start in range(0, len(records), settings.prompts):
        batch = records[start : start + settings.prompts]
        texts = [
            tokenizer.apply_chat_template(
                r["messages"][:-1], add_generation_prompt=True, tokenize=False
            )
            for r in batch
        ]
        inputs = tokenizer(texts, return_tensors="pt", padding=True).to(model.device)
        with torch.no_grad():
            output = model.generate(
                **inputs,
                do_sample=True,
                temperature=settings.temperature,
                top_p=1.0,
                max_new_tokens=settings.completion,
                num_return_sequences=settings.answers,
                pad_token_id=tokenizer.pad_token_id or tokenizer.eos_token_id,
            )
        new = output[:, inputs["input_ids"].shape[1] :]
        decoded = tokenizer.batch_decode(new, skip_special_tokens=True)
        for i in range(len(batch)):
            found.append(decoded[i * settings.answers : (i + 1) * settings.answers])
    return found


def load(model: str, revision: str, adapters: list[Path]) -> tuple[object, object]:
    """The base model with `adapters` merged in order, on the GPU in bfloat16, and its tokenizer."""
    import torch
    from peft import PeftModel
    from transformers import AutoModelForCausalLM, AutoTokenizer

    merged = AutoModelForCausalLM.from_pretrained(model, revision=revision, dtype=torch.bfloat16)
    for adapter in adapters:
        merged = PeftModel.from_pretrained(merged, str(adapter)).merge_and_unload()
    tokenizer = AutoTokenizer.from_pretrained(model, revision=revision)
    return merged.to("cuda").eval(), tokenizer


def load_merged(path: Path) -> tuple[object, object]:
    """A merged model saved with its tokenizer, on the GPU in bfloat16."""
    import torch
    from transformers import AutoModelForCausalLM, AutoTokenizer

    model = AutoModelForCausalLM.from_pretrained(str(path), dtype=torch.bfloat16)
    return model.to("cuda").eval(), AutoTokenizer.from_pretrained(str(path))


def validated(path: Path, records: list[dict], settings: Settings) -> dict:
    """pass@1, pass@4 and pass@8 of the merged model at `path` on `settings.records` of `records`,
    eight answers each at temperature 1.0, judged by the compiler (R5.1); the GPU freed after."""
    import torch

    model, tokenizer = load_merged(path)
    try:
        chosen = drawn(records, settings.records, settings.seed)
        rows = judged(chosen, generate(model, tokenizer, chosen, settings), settings.workers)
    finally:
        del model
        torch.cuda.empty_cache()
    return {"records": len(rows), **pass_rates(rows, ks=(1, 4, 8))}


def sample(
    model: object, tokenizer: object, records: list[dict], settings: Settings, out: Path
) -> list[dict]:
    """Sample and judge every drawn record, writing the rows to `out` as JSON lines, each with
    its record's index among `records`; the rows."""
    indices = drawn(list(range(len(records))), settings.records, settings.seed)
    chosen = [records[i] for i in indices]
    rows = judged(chosen, generate(model, tokenizer, chosen, settings), settings.workers)
    for index, row in zip(indices, rows, strict=True):
        row["index"] = index
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(
        "".join(json.dumps(r, ensure_ascii=False) + "\n" for r in rows), encoding="utf-8"
    )
    return rows
