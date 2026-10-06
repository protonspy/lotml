"""Fine-tuning the guide: a LoRA adapter on the base model, trained on the guidance records with
the loss on the answer alone, merged, and exported as a quantized GGUF the guide server loads
(plans/harness-guide.md 2.2).

    uv run --group train python -m lotml_harness.guide.train --records <dir> --out <dir>
    uv run --group train python -m lotml_harness.guide.train --export <out> --llama-cpp <dir>

The heavy libraries are imported where they are used, so the records' handling is tested without
them.
"""

import argparse
import json
import os
import subprocess
import sys
import time
from dataclasses import asdict, dataclass
from pathlib import Path

from lotml_harness import split

BUCKETS = ("train", "validation")


class HeldOut(ValueError):
    """A held-out record among the training ones: the evaluation would see its own data."""


@dataclass(frozen=True)
class Settings:
    """What a run is trained with; the report keeps them beside its numbers."""

    model: str = "Qwen/Qwen2.5-Coder-0.5B-Instruct"
    rank: int = 16
    alpha: int = 32
    dropout: float = 0.05
    learning_rate: float = 2e-4
    epochs: float = 1.0
    batch: int = 1
    accumulate: int = 16
    max_length: int = 4096
    warmup_steps: int = 10
    seed: int = 0
    checkpoint_steps: int = 50
    memory_fraction: float = 0.85
    """The card's share the CUDA allocator may hold: past it the Windows driver spills into system
    memory, slowing every step and growing the process (adr:0016)."""


def load(records: Path, bucket: str) -> list[dict]:
    """One split's records, every one's problem checked again against the split."""
    path = records / f"{bucket}.jsonl"
    found = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]
    for record in found:
        problem = record["meta"]["problem"]
        if split.split(problem) != bucket:
            raise HeldOut(f"{problem} is {split.split(problem)}, not {bucket}")
    return found


def examples(records: list[dict]) -> list[dict]:
    """Records as prompt and completion: the system message and the state as the prompt, the
    answer as the completion, so the loss falls on the answer alone."""
    return [{"prompt": r["messages"][:-1], "completion": r["messages"][-1:]} for r in records]


def train(settings: Settings, records: Path, out: Path) -> dict:
    """Train the adapter on the train split and write it to `out/adapter`; the run's report."""
    os.environ.setdefault(
        "PYTORCH_CUDA_ALLOC_CONF", "garbage_collection_threshold:0.6,max_split_size_mb:256"
    )
    import torch
    from datasets import Dataset
    from peft import LoraConfig
    from transformers import AutoModelForCausalLM, AutoTokenizer
    from trl import SFTConfig, SFTTrainer

    torch.cuda.set_per_process_memory_fraction(settings.memory_fraction)
    rows = examples(load(records, "train"))
    tokenizer = AutoTokenizer.from_pretrained(settings.model)
    config = SFTConfig(
        output_dir=str(out / "checkpoints"),
        num_train_epochs=settings.epochs,
        per_device_train_batch_size=settings.batch,
        gradient_accumulation_steps=settings.accumulate,
        learning_rate=settings.learning_rate,
        lr_scheduler_type="cosine",
        warmup_steps=settings.warmup_steps,
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
    lora = LoraConfig(
        r=settings.rank,
        lora_alpha=settings.alpha,
        lora_dropout=settings.dropout,
        target_modules="all-linear",
        task_type="CAUSAL_LM",
    )
    model = AutoModelForCausalLM.from_pretrained(settings.model, dtype=torch.bfloat16)
    trainer = SFTTrainer(
        model=model,
        args=config,
        train_dataset=Dataset.from_list(rows),
        peft_config=lora,
        processing_class=tokenizer,
    )
    torch.cuda.reset_peak_memory_stats()
    started = time.time()
    resumed = bool(sorted((out / "checkpoints").glob("checkpoint-*")))
    result = trainer.train(resume_from_checkpoint=resumed)
    trainer.model.save_pretrained(out / "adapter")
    report = {
        "settings": asdict(settings),
        "examples": len(rows),
        "seconds": round(time.time() - started, 1),
        "loss": result.training_loss,
        "peak_gpu_gib": round(torch.cuda.max_memory_allocated() / 2**30, 2),
        "gpu": torch.cuda.get_device_name(0),
    }
    (out / "train.json").write_text(json.dumps(report, indent=2), encoding="utf-8")
    return report


def merge(model: str, adapter: Path, out: Path) -> Path:
    """The base model with the adapter merged into it, saved with its tokenizer."""
    import torch
    from peft import PeftModel
    from transformers import AutoModelForCausalLM, AutoTokenizer

    base = AutoModelForCausalLM.from_pretrained(model, dtype=torch.bfloat16)
    PeftModel.from_pretrained(base, str(adapter)).merge_and_unload().save_pretrained(out)
    AutoTokenizer.from_pretrained(model).save_pretrained(out)
    return out


def export(merged: Path, llama_cpp: Path, out: Path, quantization: str = "Q4_K_M") -> Path:
    """The merged model as a GGUF file, quantized for the CPU with llama.cpp's own tools: its
    converter from the source tree and `llama-quantize` from its release."""
    full = out / "guide-f16.gguf"
    quantized = out / f"guide-{quantization.lower()}.gguf"
    converter = next(llama_cpp.rglob("convert_hf_to_gguf.py"))
    quantize = next(p for p in llama_cpp.rglob("llama-quantize*") if p.suffix in ("", ".exe"))
    subprocess.run(  # noqa: S603
        [sys.executable, str(converter), str(merged), "--outfile", str(full), "--outtype", "f16"],
        check=True,
    )
    subprocess.run([str(quantize), str(full), str(quantized), quantization], check=True)  # noqa: S603
    return quantized


def main(argv: list[str] | None = None) -> None:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--records", type=Path)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--model", default=Settings.model)
    parser.add_argument("--export", action="store_true", help="merge and quantize a trained run")
    parser.add_argument("--llama-cpp", type=Path, help="llama.cpp's source and release")
    parser.add_argument("--quantization", default="Q4_K_M")
    args = parser.parse_args(argv)
    if args.export:
        merged = merge(args.model, args.out / "adapter", args.out / "merged")
        print(export(merged, args.llama_cpp, args.out, args.quantization))
        return
    print(json.dumps(train(Settings(model=args.model), args.records, args.out), indent=2))


if __name__ == "__main__":
    main()
