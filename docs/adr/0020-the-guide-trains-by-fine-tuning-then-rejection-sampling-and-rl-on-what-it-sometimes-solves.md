---
status: accepted
---

# 0020 · The guide trains by fine-tuning, then rejection sampling, and RL only on what it sometimes solves

## Context

The training pipeline (specs/training-pipeline/) first ran end to end on RunPod on 2026-10-07. It
fine-tuned the 0.5B guide on 4272 seeded records, then trained it by group-relative policy
optimization on 1024 of them. That second stage gave nothing:

- 95% of the groups of eight answers scored alike by step 50, and such a group carries no
  gradient.
- The KL to the starting model stayed at 0.0000.
- On the 804 validation records, the share of edits that passed went from 80.3% to 79.6%.

The supervised guide already solves most seeded training records. Its weakness, edits that copy the
broken body, showed on real failures.

The survey merged the same day (plans/small-coder-training.md) reads that result the way the
literature does:

- **No gradient where nothing varies.** A group that agrees carries no gradient
  (docs/wiki/pages/grpo.md). A model that cannot sample a correct answer, or never fails, has
  nothing for RL to reinforce.
- **The pool is calibrated on the model being trained.** It keeps the tasks the model solves
  sometimes, from harder seeded tasks (docs/wiki/pages/rl-environment.md,
  docs/wiki/pages/repair-training.md).
- **Localizers train well by fine-tuning.** Most of SoRFT's gain came from rejection-sampled
  fine-tuning (7.6 to 18.0 resolved), and RL then took it to 21.4
  (docs/wiki/pages/verifiable-rewards.md).
- **A hit reward gets gamed.** One that pays for listing the right location among others was gamed
  by listing more. The guide's reward should be an F-score over the declarations it names.
- **Plain GRPO settings.**
  - One update per batch.
  - No KL, or a very small one.
  - An upper clip of 0.25–0.28.
  - Temperature near 1.0.
  - A loss that does not divide by answer length, with no division by the group's deviation.
  - FP16 rather than BF16 for LoRA, since a BF16 LoRA run collapsed after about 600 steps.
  - The best checkpoint kept, not the last.
- **Controls on every run.** Each run gets a random-reward twin and a pass@k curve, and a seeded
  score does not rank models on real bugs (RealiT).
- **Distillation needs a teacher that knows the language.** No teacher knows lotml. A distilling
  teacher would have to be fine-tuned on lotml first, and a smaller teacher may teach a 0.5B student
  better (docs/wiki/pages/small-coder-training.md).

## Decision

The guide is trained in this order, each stage in the training pipeline:

1. **Seeded failures that are harder:** two mutations in one program as well as one, each kept only
   when the mutated program fails and its known fix passes.
2. **Supervised fine-tuning** on the records, as before.
3. **Sampling.** The fine-tuned guide answers each train record several times at temperature 1.0,
   and the compiler judges every answer.
4. **Rejection-sampled fine-tuning** on the answers whose edit passes and whose first location is
   right, the record's own target standing in where none did.
5. **A calibrated pool:** the records whose answers' mean reward lies strictly between zero and one,
   with a small share of the always-solved ones.
6. **Group-relative policy optimization on that pool only.**
   - *Settings:* loss `dr_grpo`, rewards not scaled by the group, KL 0, clip 0.2 below and 0.28
     above, temperature 1.0, FP16, a learning rate of 1e-5 for the LoRA adapter.
   - *Reward:* an F-score with beta 3 over the declarations an answer names, zero when one of them
     is not in the file, averaged with whether its edit passes.
   - *Checkpoints:* the best one on a validation sample is kept.
   - *Control:* a twin run with a random reward is the comparison.
7. **Export,** with pass@1, pass@4 and pass@8 sampled on the validation split beside the tool's own
   answers.

The seeded validation split is a smoke test; held-out real failures decide (specs/guide-evaluation/).
The other options were these:

- **RL on the whole train split.** It is what ran, and it learned nothing.
- **Distillation from a frontier model through its API.** An API returns text, the teacher does not
  know lotml, and its terms are held by the licence registry.
- **On-policy distillation from a local teacher.** A Qwen2.5-Coder of 1.5B or 3B fine-tuned by this
  pipeline would share the records' tokenizer. It is the next step if the 0.5B plateaus, not this
  one.

## Consequences

- The pipeline gains two stages, sampling and rejection-sampled fine-tuning. Sampling is the
  costliest of them: several answers to every train record on the GPU, each judged by the compiler
  on the pod's cores.
- The reward changes. It no longer pays half for a right declaration listed after a wrong one, so
  rewards from before this record do not compare with those after it.
- RL may find little to train on when the calibrated pool is small. That is a result, recorded
  with the pool's size, not a reason to train on the whole split again.
- Every RL run costs a second run, its random-reward twin. At the pool sizes expected it is minutes
  of GPU, and without it a gain cannot be told from noise.
- The guide still ships as Q4_K_M, and its numbers are measured on that file.
