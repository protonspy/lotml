---
autonomy: auto
ci: wait
branch: feat/training-pipeline
delivery: in-progress
---

# Training pipeline — requirements

## Purpose

The harness guide was trained once, by hand, on the local GPU (adr:0017-a-half-billion-coder-model-tuned-locally-and-served-by-llama-server).
This turns that into a pipeline that runs end to end on rented GPUs, from the guidance records to
a quantized model with its threshold, and follows the study behind the guide
(docs/wiki/pages/compiler-embedded-model.md). SLMFix trained its 0.5B fixer by supervised
fine-tuning and then by reinforcement learning against a validator, its reward the validator's
verdict averaged with similarity to the reference; Break-It-Fix-It found the critic's check
decisive; DrRepair, that the line the compiler reports is often not the one to fix. The guide
learned where to change a file by supervised fine-tuning alone, and on real failures its edits
copied the broken code (plans/harness-guide.md 2.3) — the compiler as critic is the reward that
targets that. Every GPU hour is paid, so the pipeline prices a run before starting it, holds it to
a cap, and never leaves a pod running.

## R1 · Pods

- **R1.1** The training pipeline shall run each GPU stage in a RunPod pod it creates through RunPod's REST API, with the key read from `RUNPOD_API_KEY` and written to no file, log or report.
- **R1.2** If a run's estimated cost — the GPU's hourly price from RunPod's catalog times the run's deadline — added to the spend already recorded exceeds the cap, then the training pipeline shall refuse to create its pod.
- **R1.3** When a pod ends, the training pipeline shall record in a committed ledger the run, the stages, the GPU, the hourly price, the start and end times, the cost, and the total spent against the cap.
- **R1.4** When a run finishes, fails, or reaches its deadline, the training pipeline shall terminate its pod and confirm through the API that the pod is gone.
- **R1.5** If the machine that started a pod stops watching it, then the training pipeline shall terminate that pod from inside it at the run's deadline.
- **R1.6** The training pipeline shall run a stage from the repository's code at one commit, recorded with the run.

## R2 · Artifacts

- **R2.1** The training pipeline shall keep every run's inputs and outputs — the records, checkpoints, adapters, merged models, GGUF files and reports — in a private Hugging Face repository, with the token read from `HF_TOKEN` and written to no file, log or report.
- **R2.2** If the Hugging Face repository is not private, then the training pipeline shall write nothing to it.
- **R2.3** While a stage trains, the training pipeline shall upload each checkpoint it saves, and when a stage starts with a checkpoint of its run in the repository, the training pipeline shall resume from it.
- **R2.4** The training pipeline shall record with each run the digest of the records it read and the run each of its inputs came from, so a GGUF file's lineage back to its records can be read from the repository alone.
- **R2.5** If the records a run is given hold a problem the split holds out, then the training pipeline shall refuse to run it.

## R3 · Stages

- **R3.1** The training pipeline shall build the guidance records on the local machine and upload them before any pod starts.
- **R3.2** The training pipeline shall fine-tune the base model on the train split's records with the loss on the answer alone.
- **R3.3** The training pipeline shall then train the fine-tuned model by reinforcement learning with group-relative policy optimization, sampling several answers to each train record and scoring each with the reward of R4.
- **R3.4** The training pipeline shall merge the trained adapter into the base model, export it as a Q4_K_M GGUF file with llama.cpp build b11450, and calibrate the threshold on the validation split for a target precision.
- **R3.5** Where a run names stages, the training pipeline shall run only those, each from the outputs of the run it names for its input.

## R4 · The reward

- **R4.1** The training pipeline shall score an answer outside the answer schema zero.
- **R4.2** The training pipeline shall score any other answer as the mean of two parts: its locations — one when the first names a declaration the real fix changed, one half when only a later one does, else zero — and its edit — one when the edit, made as the `guide` tool's gate makes it, leaves the file checking clean and, for a failing test block, makes that block pass, else zero.
- **R4.3** If judging an answer passes its deadline or the compiler fails, then the training pipeline shall score that answer zero and count it in the run's report.
- **R4.4** The training pipeline shall judge every answer in a scratch copy, never in the repository or the records.

## R5 · Reports

- **R5.1** When a run ends, the training pipeline shall commit its report: the stages, their settings, the records' digest, the GPU, the time and the cost, and on the validation split, before and after reinforcement learning, the share of answers within the schema, top-1 and top-3 location, the share of edits that pass, and the calibrated threshold with its precision.
- **R5.2** The training pipeline shall hold no key, token or path under the user's home in the ledger or in a report.

## Out of scope

- Self-training on real broken code, Break-It-Fix-It's rounds: real failures on train problems come
  from agent traces, and the trace dataset exports none until the models' terms are verified
  (plans/agent-data.md 1.1).
- The verdict on held-out real failures: specs/guide-evaluation/.
- Serving the guide from RunPod: it is served on the user's machine (adr:0017-a-half-billion-coder-model-tuned-locally-and-served-by-llama-server).
- More than one GPU per run.
- Distillation. The supervised targets come from the compiler — the diff between the failing file
  and its real fix — so they are always right and no teacher bounds them. The study found a
  teacher that does not know the language passes on its explanations and not its fixes (10.0%
  correct code on low-resource languages), and distillation giving nothing at 100M parameters
  (docs/wiki/pages/compiler-embedded-model.md). A frontier teacher's outputs are also held by the
  licence registry. Distilling a larger Qwen2.5-Coder guide trained by this pipeline into the 0.5B
  is the case left open, for when the larger one wins on held-out real failures and the 0.5B is
  needed for its latency on the CPU.
