"""Reinforcement learning of the guide by group-relative policy optimization: the guide merged into
its base model, a fresh LoRA adapter trained on answers sampled to the records it sometimes solves
and scored by the compiler (specs/training-pipeline/ R3.3, R3.8, R3.9, R4; adr:0020).

The settings follow the survey (docs/wiki/pages/grpo.md): Dr. GRPO's loss, which divides by
neither an answer's length nor its group's spread; no KL; an upper clip of 0.28; one update per
batch of samples at temperature 1.0; FP16, in which a LoRA run stayed stable where BF16 collapsed;
and the checkpoint best on a validation sample kept rather than the last.

The heavy libraries are imported where they are used, so the pool and the dataset are tested
without them.
"""

import json
import random
import shutil
import time
from dataclasses import asdict, dataclass
from pathlib import Path

from lotml_harness.guide import sample, train
from lotml_harness.guide.reward import Reward


@dataclass(frozen=True)
class Settings:
    """What a reinforcement-learning run is trained with, kept in its report."""

    model: str = train.Settings.model
    revision: str = train.Settings.revision
    rank: int = 16
    alpha: int = 32
    dropout: float = 0.05
    learning_rate: float = 1e-5
    """For a LoRA adapter: full fine-tuning recipes use 1e-6, and at 1e-6 with KL 0.04 the first run
    left the policy where it began."""
    beta: float = 0.0
    epsilon: float = 0.2
    epsilon_high: float = 0.28
    loss: str = "dr_grpo"
    scale: str = "none"
    temperature: float = 1.0
    generations: int = 8
    """Answers sampled per record: the group whose mean reward is each answer's baseline."""
    completion: int = 1024
    """The guide tool's answer budget."""
    prompts: int = 1024
    """At most this many pool records, drawn by `seed`, for one epoch."""
    easy: float = 0.1
    """The always-solved records added to the pool, as a share of the sometimes-solved ones."""
    evaluated: int = 64
    """Validation records the checkpoints are compared on."""
    batch: int = 8
    accumulate: int = 2
    seed: int = 0
    checkpoint_steps: int = 50
    workers: int = 8
    """Answers judged at once, each a `lotml` process."""
    random_reward: bool = False
    """The control: a reward drawn at random in place of the compiler's (R3.9)."""


def pool(rows: list[dict], easy: float, seed: int) -> list[int]:
    """The records reinforcement learning trains on, by index: those whose sampled answers' mean
    reward lies strictly between zero and one — the guide solves them sometimes — and, drawn by
    `seed`, a share of the always-solved ones (R3.8). The never-solved carry no reward to learn
    from and are left out."""
    sometimes = [r["index"] for r in rows if 0.0 < r["mean"] < 1.0]
    always = sorted(r["index"] for r in rows if r["mean"] >= 1.0)
    extra = min(len(always), round(easy * len(sometimes)))
    chosen = random.Random(seed).sample(always, extra)  # noqa: S311 - a reproducible draw
    return sorted(set(sometimes) | set(chosen))


def rows(records: list[dict], indices: list[int], count: int, seed: int) -> list[dict]:
    """At most `count` of the records at `indices`, drawn by `seed`, as GRPO's dataset: the prompt
    is the system and user messages; `state` and `truth`, as JSON text, are the columns the reward
    reads — the state the record was rendered from, and the symbols the real fix changed."""
    chosen = sorted(sample.index(i, len(records)) for i in indices)
    if len(chosen) > count:
        chosen = sorted(random.Random(seed).sample(chosen, count))  # noqa: S311 - a reproducible draw
    return [
        {
            "prompt": records[i]["messages"][:-1],
            "state": json.dumps(records[i]["state"]),
            "truth": json.dumps(
                [
                    loc["symbol"]
                    for loc in json.loads(records[i]["messages"][-1]["content"])["locations"]
                ]
            ),
        }
        for i in chosen
    ]


class RandomReward:
    """The control twin's reward: one or zero at even odds, whatever the answer (R3.9)."""

    __name__ = "random"

    def __init__(self, seed: int = 0) -> None:
        self.draw = random.Random(seed)  # noqa: S311 - a control, not a secret

    def __call__(self, completions: list, **_: object) -> list[float]:
        return [float(self.draw.random() < 0.5) for _ in completions]


def summary(history: list[dict]) -> dict:
    """What the log says of a run: the reward at its start and end, the share of groups whose
    answers all scored alike, entropy at the start and end, the share of answers cut off, and the
    validation reward at each evaluation."""

    def values(key: str) -> list[float]:
        return [e[key] for e in history if key in e]

    rewards, alike = values("reward"), values("frac_reward_zero_std")
    entropy, clipped = values("entropy"), values("completions/clipped_ratio")
    return {
        "reward_first": rewards[0] if rewards else None,
        "reward_last": rewards[-1] if rewards else None,
        "alike": sum(alike) / len(alike) if alike else None,
        "entropy_first": entropy[0] if entropy else None,
        "entropy_last": entropy[-1] if entropy else None,
        "clipped": sum(clipped) / len(clipped) if clipped else None,
        "eval_rewards": values("eval_reward"),
    }


def best_step(history: list[dict]) -> int | None:
    """The step whose evaluation on the validation sample scored the highest reward, the later one
    on a tie; None when the run was never evaluated. TRL logs the evaluation's reward but does not
    hand it to the trainer's own choice of a best model, so the run keeps every checkpoint and
    this picks one."""
    evaluated = [
        (float(e["eval_reward"]), e["step"])
        for e in history
        if "eval_reward" in e and type(e.get("step")) is int
    ]
    return max(evaluated)[1] if evaluated else None


def kept(out: Path, best: int | None) -> Path | None:
    """The checkpoint directory holding step `best`'s adapter, inside `out/checkpoints`; None when
    the run was never evaluated. RuntimeError when that checkpoint lacks the adapter, rather than
    keeping another step's adapter under the best one's name."""
    if best is None:
        return None
    root = (out / "checkpoints").resolve()
    found = (root / f"checkpoint-{int(best)}").resolve()
    if not found.is_relative_to(root) or not all((found / name).exists() for name in ADAPTER):
        raise RuntimeError(f"checkpoint-{best}, the best on validation, holds no adapter")
    return found


ADAPTER = ("adapter_config.json", "adapter_model.safetensors")
"""What a checkpoint holds of the adapter, copied out as the run's adapter."""


def fit(
    settings: Settings,
    records: Path,
    samples: list[dict],
    adapter: Path,
    out: Path,
    callbacks: list | None = None,
) -> dict:
    """Train a fresh adapter on top of `adapter`, merged, on the pool the `samples` calibrate; save
    to `out/adapter` the checkpoint best on a validation sample, every checkpoint kept to choose
    from, and return the run's report."""
    import torch
    from datasets import Dataset
    from peft import LoraConfig, PeftModel
    from transformers import AutoModelForCausalLM, AutoTokenizer
    from trl import GRPOConfig, GRPOTrainer

    found = train.load(records, "train")
    chosen = pool(samples, settings.easy, settings.seed)
    data = rows(found, chosen, settings.prompts, settings.seed)
    validation = train.load(records, "validation")
    evaluated = rows(validation, list(range(len(validation))), settings.evaluated, settings.seed)
    tokenizer = AutoTokenizer.from_pretrained(settings.model, revision=settings.revision)
    base = AutoModelForCausalLM.from_pretrained(
        settings.model, revision=settings.revision, dtype=torch.float16
    )
    model = PeftModel.from_pretrained(base, str(adapter)).merge_and_unload()
    scorer = RandomReward(settings.seed) if settings.random_reward else Reward(settings.workers)
    config = GRPOConfig(
        output_dir=str(out / "checkpoints"),
        num_train_epochs=1,
        per_device_train_batch_size=settings.batch,
        per_device_eval_batch_size=settings.generations,
        gradient_accumulation_steps=settings.accumulate,
        num_generations=settings.generations,
        max_completion_length=settings.completion,
        learning_rate=settings.learning_rate,
        beta=settings.beta,
        epsilon=settings.epsilon,
        epsilon_high=settings.epsilon_high,
        loss_type=settings.loss,
        scale_rewards=settings.scale,
        temperature=settings.temperature,
        num_iterations=1,
        fp16=True,
        bf16=False,
        gradient_checkpointing=True,
        logging_steps=10,
        eval_strategy="steps",
        eval_steps=settings.checkpoint_steps,
        save_strategy="steps",
        save_steps=settings.checkpoint_steps,
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
        reward_funcs=[scorer],
        args=config,
        train_dataset=Dataset.from_list(data),
        eval_dataset=Dataset.from_list(evaluated),
        peft_config=lora,
        processing_class=tokenizer,
        callbacks=callbacks,
    )
    torch.cuda.reset_peak_memory_stats()
    started = time.time()
    resumed = bool(sorted((out / "checkpoints").glob("checkpoint-*")))
    trainer.train(resume_from_checkpoint=resumed)
    best = best_step(trainer.state.log_history)
    chosen_checkpoint = kept(out, best)
    if chosen_checkpoint is not None:
        (out / "adapter").mkdir(parents=True, exist_ok=True)
        for name in ADAPTER:
            shutil.copy2(chosen_checkpoint / name, out / "adapter" / name)
    else:
        trainer.model.save_pretrained(out / "adapter")
    rewards = {e["step"]: e["eval_reward"] for e in trainer.state.log_history if "eval_reward" in e}
    return {
        "settings": asdict(settings),
        "pool": len(chosen),
        "prompts": len(data),
        "sampled": len(samples),
        "always": sum(1 for r in samples if r["mean"] >= 1.0),
        "never": sum(1 for r in samples if r["mean"] <= 0.0),
        "best": None if best is None else f"checkpoint-{best}",
        "best_metric": rewards.get(best),
        "seconds": round(time.time() - started, 1),
        **summary(trainer.state.log_history),
        "unjudged": getattr(scorer, "unjudged", 0),
        "peak_gpu_gib": round(torch.cuda.max_memory_allocated() / 2**30, 2),
        "gpu": torch.cuda.get_device_name(0),
    }
