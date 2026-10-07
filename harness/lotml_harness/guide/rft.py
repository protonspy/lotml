"""Rejection-sampled fine-tuning of the guide: the supervised adapter trained further on the guide's
own sampled answers that the compiler passed, the record's target where none did
(specs/training-pipeline/ R3.7, adr:0020). SoRFT drew most of its gain from this stage, ahead of
reinforcement learning (docs/wiki/pages/verifiable-rewards.md).

The heavy libraries are imported where they are used, so the examples are tested without them.
"""

import time
from dataclasses import asdict, dataclass
from pathlib import Path

from lotml_harness.guide import sample, train


@dataclass(frozen=True)
class Settings:
    """What rejection-sampled fine-tuning is trained with; the report keeps them."""

    keep: int = 2
    """Passing answers kept per record, distinct ones only."""
    learning_rate: float = 1e-4
    epochs: float = 1.0
    batch: int = 1
    accumulate: int = 16
    max_length: int = 4096
    seed: int = 0
    checkpoint_steps: int = 50


def examples(records: list[dict], rows: list[dict], keep: int) -> tuple[list[dict], dict]:
    """Prompt and completion examples: per sampled record, up to `keep` distinct answers that
    passed — first location right and edit passing — or, when none did, the record's own target;
    and the counts of each."""
    found, counts = [], {"answers": 0, "targets": 0, "records": len(rows)}
    for row in rows:
        record = records[row["index"]]
        texts: list[str] = []
        for answer in row["answers"]:
            if sample.passed(answer) and answer["text"] not in texts:
                texts.append(answer["text"])
        texts = texts[:keep]
        if texts:
            counts["answers"] += len(texts)
        else:
            texts = [record["messages"][-1]["content"]]
            counts["targets"] += 1
        found += [
            {
                "prompt": record["messages"][:-1],
                "completion": [{"role": "assistant", "content": text}],
            }
            for text in texts
        ]
    return found, counts


def fit(
    settings: Settings,
    rows: list[dict],
    records: Path,
    adapter: Path,
    out: Path,
    callbacks: list | None = None,
) -> dict:
    """Train the supervised adapter further on the examples, into `out/adapter`; the report."""
    import torch
    from datasets import Dataset
    from peft import PeftModel
    from transformers import AutoModelForCausalLM, AutoTokenizer
    from trl import SFTConfig, SFTTrainer

    base = train.Settings()
    found, counts = examples(train.load(records, "train"), rows, settings.keep)
    tokenizer = AutoTokenizer.from_pretrained(base.model, revision=base.revision)
    model = AutoModelForCausalLM.from_pretrained(
        base.model, revision=base.revision, dtype=torch.bfloat16
    )
    model = PeftModel.from_pretrained(model, str(adapter), is_trainable=True)
    config = SFTConfig(
        output_dir=str(out / "checkpoints"),
        num_train_epochs=settings.epochs,
        per_device_train_batch_size=settings.batch,
        gradient_accumulation_steps=settings.accumulate,
        learning_rate=settings.learning_rate,
        lr_scheduler_type="cosine",
        warmup_steps=10,
        bf16=True,
        gradient_checkpointing=True,
        max_length=settings.max_length,
        logging_steps=20,
        save_strategy="steps",
        save_steps=settings.checkpoint_steps,
        save_total_limit=2,
        report_to=[],
        seed=settings.seed,
    )
    trainer = SFTTrainer(
        model=model,
        args=config,
        train_dataset=Dataset.from_list(found),
        processing_class=tokenizer,
        callbacks=callbacks,
    )
    started = time.time()
    resumed = bool(sorted((out / "checkpoints").glob("checkpoint-*")))
    result = trainer.train(resume_from_checkpoint=resumed)
    trainer.model.save_pretrained(out / "adapter")
    return {
        "settings": asdict(settings),
        "examples": len(found),
        **counts,
        "seconds": round(time.time() - started, 1),
        "loss": result.training_loss,
        "gpu": torch.cuda.get_device_name(0),
    }
