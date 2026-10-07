# GRPO

Group Relative Policy Optimization, the reinforcement-learning algorithm behind most open recipes
for training code models against a verifier. This page covers what it computes, the fixes its
variants made, the failure modes to watch, and the settings small-model runs used. Whether to run it
at all is [[small-coder-training]]; what to reward is [[verifiable-rewards]]. Every number taken
from a paper in `research/literature/sources.json` is quoted in `claims.json` there
([[source-verification]]).

## What it computes

- **No critic.** For each prompt GRPO samples a group of answers and uses the group's own scores as
  the baseline: each answer's reward minus the group mean, divided by the group's standard
  deviation, is the advantage of every one of its tokens
  ([DeepSeekMath](https://arxiv.org/abs/2402.03300)).
- **A group that agrees teaches nothing.** When every answer in a group gets the same reward the
  advantages are all zero. With a binary reward, 8 samples and one success, the success gets +2.65
  and each failure −0.38 (mean 0.125, standard deviation 0.331): a rare success is amplified.
- **As first defined** it kept a KL penalty against a reference model in the loss, with coefficient
  0.04, and scored answers with a learned reward model; RL improved majority voting and not pass@k.
  The recipes below dropped or changed most of that.

## The fixes

- **DAPO** ([arXiv 2503.14476](https://arxiv.org/abs/2503.14476)) raised the upper clip bound to 0.28
  with the lower at 0.2, dropped groups whose answers were all right or all wrong, averaged the loss
  over tokens instead of over answers, and masked truncated answers instead of punishing them. On a
  32B base, AIME24 went from 30 to 50 as these were added in a fixed order: masking 36, clip-higher
  38, a soft penalty past the length limit 41, token-level loss 42, dynamic sampling 50. Masking and
  then soft punishment, not masking alone, is the recipe; all of it is math with integer answers,
  and DAPO took 16 gradient updates per batch of samples.
- **Dr. GRPO** ([arXiv 2503.20783](https://arxiv.org/abs/2503.20783)) found two biases. Dividing each
  answer's loss by its length punishes long wrong answers less than short ones, so wrong answers grow;
  dividing by the group's standard deviation over-weights prompts that are almost always solved or
  almost never. It removes both and divides by a constant. At the time every open implementation the
  authors examined, trl and verl among them, normalized by response length: which loss a framework
  computes has to be checked.
- **The variant matters less than it seems.** On one 7B math run, PPO, GRPO, Reinforce++, RLOO,
  ReMax and DAPO left similar gaps to the base model's large-k pass rate, from 43.9 for GRPO to 42.6
  for RLOO; DAPO's dynamic sampling needed about 3–6× more samples per batch
  ([Yue et al.](https://arxiv.org/abs/2504.13837)). A KL term of 0.001 left pass@1 alone and lowered
  pass@128; more samples per prompt, 8 to 32, raised pass@128. In VeRPO's ablation, dividing by the
  group's standard deviation did worse than a fixed factor ([VeRPO](https://arxiv.org/abs/2601.03525)).
- **Newer variants** for mixture-of-experts models and asynchronous pipelines —
  [GSPO](https://arxiv.org/abs/2507.18071), [CISPO](https://arxiv.org/abs/2506.13585) — fix problems a
  dense 1.5B model trained nearly on-policy does not have.

## What goes wrong, and what to log

- **Entropy collapse.** More gradient steps per batch of samples sped up the collapse, and sampling at
  temperature 0.6 sent the policy to low entropy at once on math and quickly on code, so
  [Skywork-OR1](https://arxiv.org/abs/2505.22312) samples at 1.0, which scores lower early and higher
  at the end. AceReason saw 2 or 4 updates per batch collapse entropy around step 100 and one update
  prevent it ([arXiv 2505.16400](https://arxiv.org/abs/2505.16400)). Clip-higher cuts both ways: in an
  off-policy run DAPO's 0.28 made entropy climb with poor test results while 0.25 and 0.265 worked
  (Skywork-OR1), and with a DAPO-style loss the upper bound sets where the reward plateaus
  ([ScaleRL](https://arxiv.org/abs/2510.13786)). Entropy fell in every ScaleRL run without predicting
  the outcome: log it, do not steer by it.
- **Truncation.** Truncation rates of 10–15% destabilized ScaleRL's runs at batch 768. Skywork-OR1
  tried excluding truncated answers from the group baseline and watched the truncation ratio climb —
  a hack — and ended by giving truncated answers negative advantages, with no mask. Log the
  truncation rate.
- **Training and inference disagree.** Samples come from the inference engine and gradients from the
  trainer; ignoring the numerical difference biases the gradient, so on-policy training is in fact
  off-policy ([FP16](https://arxiv.org/abs/2510.26788)). Offline, on a distilled 1.5B model, BF16's
  mismatch was about 24 times FP16's; a LoRA GRPO run of Qwen2.5-Math-1.5B collapsed after roughly 600
  steps in BF16 and stayed stable in FP16. A full-precision output layer alone did not prevent
  collapse in that paper; in ScaleRL it lifted the plateau of a pre-ScaleRL baseline from 0.52 to
  0.61 and made little difference inside the final recipe on the dense 8B model. Importance-sampling
  corrections cost about 25% more compute. The mismatch grew before the runs that collapsed: log it.
- **Groups with nothing to learn.** 60–70% of groups had no reward variance under binary GRPO in
  VeRPO's multi-turn setting; [[rl-environment]] is how a task pool keeps that down.
- **Drift.** A multilingual 1.5B model drifted into other languages after 150–200 steps despite an
  English-only instruction ([Open-RS](https://arxiv.org/abs/2503.16219)); ProRL's answers grew
  because the model stopped terminating, and it added a penalty for answers that do not end
  ([ProRL](https://arxiv.org/abs/2505.24864)). For lotml the drift to watch is toward Python.
- **Short runs mislead.** Recipes that look better at small budgets can be worse at large ones, and
  three identical ScaleRL runs varied by up to ±0.015 in fitted plateau. Training reward is not
  progress either: DAPO's final training reward often had little correlation with validation
  accuracy.

## Settings small runs used

| setting | values reported |
| --- | --- |
| learning rate | 1e-6 in most code recipes (Seed-Coder, VeRPO, the pass-rate study), in AceReason's math stage and in Open-RS and Tina's LoRA run; 5e-7 for AceCoder, KodCode and HardTests; 5e-6 for Agnostics; 4e-5 for the FP16 paper's LoRA run |
| samples per prompt | 4 (Tina), 6 (Open-RS), 8 rising to 16 (AceReason), 16 (pass-rate study, KodCode), 32 (Agnostics: 16 slightly worse, 64 no better and about 70% longer); at a fixed batch ScaleRL found the split between prompts and samples second-order |
| updates per batch of samples | one (AceReason, the pass-rate study); 16 for DAPO and MiMo, whose later release moved to on-policy because plain GRPO stalled |
| KL | 0 (AceReason, Seed-Coder, VeRPO, HardTests); 0.001 (KodCode, Arctic-Text2SQL); kept, with periodic resets, by ProRL from a distilled model |
| clip | 0.2 / 0.28 (DAPO, Seed-Coder, VeRPO, SWE-AGILE); 0.2 / 0.4 (ProRL); 0.25–0.265 off-policy (Skywork-OR1) |
| temperature | 1.0 (Skywork-OR1, VeRPO, the pass-rate study); 0.7 (Agnostics, Open-RS); 0.6 (Seed-Coder); 1.2 (ProRL) |
| LoRA | rank 32, alpha 128 on the query, key, value and dense modules (Tina, whose ranks 8 to 32 landed within about a point); rank 32, alpha 64 on all layers (FP16 paper) |
| precision | FP16 over BF16 for LoRA GRPO |

Sources: [Seed-Coder](https://arxiv.org/abs/2506.03524), [AceCoder](https://arxiv.org/abs/2502.01718),
[KodCode](https://arxiv.org/abs/2503.02951), [HardTests](https://arxiv.org/abs/2505.24098),
[Agnostics](https://arxiv.org/abs/2508.04865), [the pass-rate study](https://arxiv.org/abs/2605.02944),
[MiMo](https://arxiv.org/abs/2505.07608), [Arctic-Text2SQL-R1](https://arxiv.org/abs/2505.20315),
[SWE-AGILE](https://arxiv.org/abs/2604.11716), [Tina](https://arxiv.org/abs/2504.15777).

## What it means for lotml

1. **Start on-policy and plain:** one update per batch of samples, no KL or a very small one, an
   upper clip between 0.25 and 0.28, temperature near 1.0, FP16, and a loss that does not divide by
   answer length — checked in the pinned trl 1.0.0
   (adr:0017-a-half-billion-coder-model-tuned-locally-and-served-by-llama-server), since Dr. GRPO
   found trl dividing by length when it was written.
2. **Log from the first step:** the share of groups with no reward variance, entropy, the truncation
   rate, the mismatch between generation and training, and held-out pass rates every few dozen steps,
   keeping the best checkpoint rather than the last.
3. **lotml's answers are short** — a median of 254 tokens for the harness guide, with 15.5% cut off at
   512 (adr:0017) — so the long-context schedules above do not apply, and truncation is a limit to set
   from the data, not a budget to grow.
4. **The variant is second-order;** the data and the reward decide ([[verifiable-rewards]],
   [[rl-environment]]).

## Not machine-checked

TRL's [GRPO trainer](https://huggingface.co/docs/trl/main/en/grpo_trainer) documents its current loss
types, KL default, clipping and vLLM options; Thinking Machines'
[LoRA Without Regret](https://thinkingmachines.ai/blog/lora/) reports LoRA matching full fine-tuning
for policy-gradient RL with a higher learning rate and adapters on every layer.
