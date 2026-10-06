---
status: accepted
---

# 0016 · A half-billion-parameter coder model, tuned locally and served by llama-server

## Context

The harness guide (plans/harness-guide.md) is a small model that tells a coding agent where to
change its lotml and what kind of change it needs, served to the compiler's `guide` tool over an
OpenAI-compatible endpoint on the user's own machine (specs/guide-tool/). Three things had to be
chosen before training it: the base model and its size, the runtime it is served from, and where
training runs. Each is expensive to undo: the records are rendered and counted with the base
model's tokenizer (specs/guide-records/), the tool's request is the runtime's dialect, and a
training setup is a stack of pinned libraries.

What held when this was decided, all measured on 2026-10-06 by a pilot on this repository's
seeded failures (specs/seeded-failures/: 4274 train and 804 validation guidance records):

- **The machine.** An NVIDIA RTX 3060 with 12 GB, an AMD Ryzen 7 7800X3D with 8 cores, 31 GB of
  memory, Windows. The guide must answer on a CPU: an agent's machine may have no GPU, and the
  tool's deadline is 75 s per call.
- **The task.** The guide points rather than writes: the study behind it found small models
  repair and locate where they cannot author (SLMFix's 0.5B fixer at 94.30% against 48.83%
  writing Ansible itself; docs/wiki/pages/compiler-embedded-model.md). The answer is short and
  constrained by a JSON schema, and the prompt is the file and the diagnostics: a median of 639
  tokens, at most 2042; answers a median of 254, at most 690.
- **The runtime's contract.** The tool asks at temperature zero with `response_format` of type
  `json_schema` and `logprobs: true`, and thresholds the probability of the answer's tokens
  through its first location. The design named this as a risk: log-probabilities under a schema
  are a runtime's feature, not the protocol's guarantee.
- **The pilot's numbers.** `Qwen/Qwen2.5-Coder-0.5B-Instruct` at revision `ea3f2471`, Apache-2.0,
  LoRA rank 16 on every linear layer, one epoch at 2e-4, the loss on the answer alone:
  - training on the RTX 3060 took about 11.6 s per step of 16 examples, some 52 minutes for the
    epoch, at a peak of 5.9 GB of GPU memory;
  - batches of 4 held logits of 4096 tokens by 152 thousand words in fp32, about 10 GB. On Windows
    the driver spilled them into system memory, which grew to 14 GB and got the run killed. One
    example per step with 16 accumulated, and the CUDA allocator held to 85% of the card, kept
    the process at 2 GB;
  - served by llama.cpp's `llama-server` (build b11450) from a Q4_K_M GGUF of 398 MB, on 8 CPU
    threads, the server took about 2 GB;
  - every answer came back valid against the schema with log-probabilities;
  - on 200 validation records the untuned model put the right declaration first 61% of the time,
    and the tuned one 100%, with answers held to 1024 tokens. At 512 tokens, 15.5% of answers
    were cut off and invalid: 14.7% of the targets are longer than 512 tokens;
  - latency was a median of 3.0 s and a 95th percentile of 9.8 s;
  - there was no marginal cost: the GPU is the user's.

## Decision

The guide is `Qwen/Qwen2.5-Coder-0.5B-Instruct`, fine-tuned with a LoRA adapter on the local
GPU, merged, quantized to Q4_K_M GGUF with llama.cpp's own converter and quantizer, and served by
llama.cpp's `llama-server` on the CPU, with answers held to 1024 tokens and records counted against
an 8192-token context. Training runs with one example per step and 16 accumulated, at most one
epoch per run, on the local RTX 3060. The other options were these:

- **The 1.5B of the same family**, under the same licence. It costs about three times the
  training time and the latency on the CPU, and the 0.5B already saturated the validation split.
  It is the next size to try if the guide fails to beat the compiler's pointer on held-out real
  failures (specs/guide-evaluation/).
- **vLLM or Ollama as the runtime.** Both serve the same request shape. vLLM wants a GPU the
  guide's user may not have. Ollama wraps llama.cpp, adds a daemon and a model registry of its
  own, and took no part in the pilot.
- **An on-demand RunPod GPU**, with checkpoints in private Hugging Face repositories. The local
  card trains the 0.5B in under an hour at no cost, so it stays the fallback for a larger model,
  with its own budget approved before a run.

## Consequences

- The training stack enters the harness in a `train` dependency group, pinned and outside the
  default environment: torch 2.11.0 built for CUDA 12.8, transformers 5.19.0, peft 0.21.2,
  trl 1.0.0, datasets 5.1.0, accelerate 1.15.0. llama.cpp is a release binary and its source's
  converter, used by path, never vendored.
- The records are counted with Qwen2.5-Coder's tokenizer. A different base family means building
  the records again; the renderer's version guards against serving a guide with prompts it was
  not trained on.
- The tool's configuration sets `answer = 1024`; at 512 the guide's longer edits are cut off and
  the tool reads them as invalid answers.
- The seeded validation split no longer discriminates: a tuned guide gets every one right. The
  numbers that decide whether the guide helps are the held-out real failures, which need agent
  runs beyond the phase 1 gate's 57 (specs/guide-evaluation/ R1.4), and the threshold calibrated
  on seeded failures alone is no evidence of precision on real ones.
- Training on Windows needs the allocator held below the card. Past that, the driver's spill into
  system memory slows a step from 17 to 30 seconds and grows the process until something kills it.
- A run is checkpointed every 50 steps and resumed from the last one, since a background process
  on a busy machine can be ended.
