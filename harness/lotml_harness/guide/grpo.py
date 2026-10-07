"""Reinforcement learning of the guide by group-relative policy optimization: the supervised guide
merged into its base model, a fresh LoRA adapter trained on answers sampled to the train split's
records and scored by the compiler (specs/training-pipeline/ R3.3, R4).

The heavy libraries are imported where they are used, so the dataset's handling is tested without
them.
"""

import json
import random
import time
from dataclasses import asdict, dataclass
from pathlib import Path

from lotml_harness.guide import train
from lotml_harness.guide.reward import Reward


@dataclass(frozen=True)
class Settings:
    """What a reinforcement-learning run is trained with, kept in its report."""

    model: str = train.Settings.model
    revision: str = train.Settings.revision
    rank: int = 16
    alpha: int = 32
    dropout: float = 0.05
    learning_rate: float = 1e-6
    beta: float = 0.04
    """The KL penalty that holds the policy near the supervised guide."""
    generations: int = 8
    """Answers sampled per record: the group whose mean reward is each answer's baseline."""
    completion: int = 1024
    """The guide tool's answer budget."""
    prompts: int = 1024
    """Train records drawn, by `seed`, for one epoch."""
    batch: int = 8
    accumulate: int = 2
    seed: int = 0
    checkpoint_steps: int = 50
    workers: int = 8
    """Answers judged at once, each a `lotml` process."""


def rows(records: list[dict], count: int, seed: int) -> list[dict]:
    """`count` records drawn by `seed` as GRPO's dataset: the prompt is the system and user
    messages; `state` and `truth`, as JSON text, are the columns the reward reads — the state the
    record was rendered from, and the symbols of the declarations the real fix changed."""
    drawn = random.Random(seed).sample(records, min(count, len(records)))  # noqa: S311 - a reproducible draw
    return [
        {
            "prompt": r["messages"][:-1],
            "state": json.dumps(r["state"]),
            "truth": json.dumps(
                [loc["symbol"] for loc in json.loads(r["messages"][-1]["content"])["locations"]]
            ),
        }
        for r in drawn
    ]


def fit(
    settings: Settings, records: Path, adapter: Path, out: Path, callbacks: list | None = None
) -> dict:
    """Train a fresh adapter on top of the supervised one, merged; write it to `out/adapter` and
    return the run's report."""
    import torch
    from datasets import Dataset
    from peft import LoraConfig, PeftModel
    from transformers import AutoModelForCausalLM, AutoTokenizer
    from trl import GRPOConfig, GRPOTrainer

    tokenizer = AutoTokenizer.from_pretrained(settings.model, revision=settings.revision)
    base = AutoModelForCausalLM.from_pretrained(
        settings.model, revision=settings.revision, dtype=torch.bfloat16
    )
    model = PeftModel.from_pretrained(base, str(adapter)).merge_and_unload()
    found = rows(train.load(records, "train"), settings.prompts, settings.seed)
    reward = Reward(workers=settings.workers)
    config = GRPOConfig(
        output_dir=str(out / "checkpoints"),
        num_train_epochs=1,
        per_device_train_batch_size=settings.batch,
        gradient_accumulation_steps=settings.accumulate,
        num_generations=settings.generations,
        max_completion_length=settings.completion,
        learning_rate=settings.learning_rate,
        beta=settings.beta,
        bf16=True,
        gradient_checkpointing=True,
        logging_steps=10,
        save_strategy="steps",
        save_steps=settings.checkpoint_steps,
        save_total_limit=2,
        report_to=[],
        seed=settings.seed,
    )
    lora = LoraConfig(
        r=settings.rank,
        lora_alpha=settings.alpha,
        lora_dropout=settings.dropout,
        target_modules="all-linear",
        task_type="CAUSAL_LM",
    )
    trainer = GRPOTrainer(
        model=model,
        reward_funcs=[reward],
        args=config,
        train_dataset=Dataset.from_list(found),
        peft_config=lora,
        processing_class=tokenizer,
        callbacks=callbacks,
    )
    torch.cuda.reset_peak_memory_stats()
    started = time.time()
    resumed = bool(sorted((out / "checkpoints").glob("checkpoint-*")))
    trainer.train(resume_from_checkpoint=resumed)
    trainer.model.save_pretrained(out / "adapter")
    rewards = [e["reward"] for e in trainer.state.log_history if "reward" in e]
    return {
        "settings": asdict(settings),
        "prompts": len(found),
        "seconds": round(time.time() - started, 1),
        "reward_first": rewards[0] if rewards else None,
        "reward_last": rewards[-1] if rewards else None,
        "unjudged": reward.unjudged,
        "peak_gpu_gib": round(torch.cuda.max_memory_allocated() / 2**30, 2),
        "gpu": torch.cuda.get_device_name(0),
    }
