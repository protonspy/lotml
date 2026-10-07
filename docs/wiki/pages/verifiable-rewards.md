# Verifiable rewards

A reward computed by a program — a compiler, a test runner, a checker — rather than by a learned
model. lotml owns both halves: `lotml check` and `lotml test`, with structured output. This page is
what the evidence says about shaping that reward, for writing code and for repairing it; the
algorithm that consumes it is [[grpo]], the machinery around it [[rl-environment]], and what a
policy does to a weak one [[reward-hacking]]. Every number taken from a paper in
`research/literature/sources.json` is quoted in `claims.json` there ([[source-verification]]).

## All tests pass, or nothing

- **The default, and its best result.** [AceCoder](https://arxiv.org/abs/2502.01718) rewarded 1 when
  every test passed and 0 otherwise. RL from Qwen2.5-Coder-7B-Base raised HumanEval+ from 53.0 to
  78.0 in 80 steps and 48 H100 hours, and the nine-benchmark average from 44.4 to 52.3, while
  LiveCodeBench stayed at 28.7 to 28.5. From the already strong instruct model the average moved 55.2
  to 55.9 and HumanEval+ fell from 86.0 to 84.8.
- **Partial credit for running cleanly lost.** [Agnostics](https://arxiv.org/abs/2508.04865), which
  rewards only programs passing every input/output test, tried 0.2 for Lua programs that ran without
  error and failed a test. Two runs scored 13.33 and 15.62 on its held-out Codeforces split against
  24.76 with the binary reward, and 18.57 and 20.16 on its LiveCodeBench port against 23.00. Its authors had dropped partial rewards after models exploited rewards for code that
  merely ran or passed only the public tests.
- **Simple rewards elsewhere.** [Arctic-Text2SQL-R1](https://arxiv.org/abs/2505.20315) rewards only
  execution correctness and basic syntax validity; other designs gave no significant improvement, and
  finer-grained ones made the model chase short-term reward. The reward ablation is one sentence, and
  the syntax credit itself was never ablated.
- **Why it works under GRPO.** Only the ranking inside a group moves the policy, and a binary reward
  makes that ranking exactly "passed or not" ([[grpo]]). Its cost is the group where nothing passes,
  which carries no gradient — the reason partial credit keeps being tried.

## Credit for tests passed: mixed

- **Below binary at convergence, in a controlled study.** [The pass-rate study](https://arxiv.org/abs/2605.02944)
  trained 768 on-policy steps with 16 samples per problem. With GRPO on DeepSeek-R1-Distill-Qwen-7B
  the fraction of tests passed finished 0.6 to 2.0 points below binary at pass@4 to pass@16 (pass@16
  55.6 against 57.6) and 0.3 above at pass@1 (40.9 against 40.6); on Qwen3-4B it lost 2.2 at pass@1
  (44.2 against 46.4) and 2.3 at pass@16. The early speed-up appeared under GRPO, not RLOO. The
  reward was genuinely dense, so density was not the problem: it rewarded brute-force and
  test-fitting partial solutions while near-correct ones that needed a small fix scored low, and in
  groups with no full pass it barely moved the probability of a correct reference. Pass rate first
  and binary later gained nothing (40.6 pass@1).
- **Ahead in a short ablation.** In [Klear-Reasoner](https://arxiv.org/abs/2508.07629) the pass rate
  scored 61.0 against 59.2 on LiveCodeBench v5 — a 100-step run from an early checkpoint, with a binary
  arm that gave a negative reward on failure. [ProRL](https://arxiv.org/abs/2505.24864) rewarded the
  fraction of tests passed, zero for code that failed to compile or ran over 5 seconds, and roughly
  doubled APPS pass@1 (20.95 to 41.99) on a distilled 1.5B model; [PyLang](https://arxiv.org/abs/2605.15607)'s
  fractional reward added under 3 points.
- **Weighted by difficulty, next to the outcome.** [VeRPO](https://arxiv.org/abs/2601.03525) weights
  each test by how rarely the group passes it, corrected for how many tests share that pass rate, and
  adds the binary outcome. With retries on execution feedback it beat binary GRPO by 2.57 on
  LiveCodeBench v6 and 8.83 on CodeElo with Qwen3-8B; single-turn, by 1.26 on average across six
  benchmarks; on Qwen3-4B, 55.86 against 52.33, setting not stated. Alone, raw pass rate fell below binary on
  LiveCodeBench v6 and CodeElo; combined with the outcome term it beat binary GRPO on all five. Test suites skew easy:
  for 53% of TACO problems, over 90% of the tests were already passed more than 80% of the time by
  Qwen3-8B before training.
  [MiMo](https://arxiv.org/abs/2505.07608) pays a difficulty level only when every test in it and in
  every easier level passes, with difficulty taken from pass rates.
- **When binary is too sparse.** A binary reward almost never positive on long generated arithmetic
  made training diverge ([arXiv 2507.10532](https://arxiv.org/abs/2507.10532)); in
  [CoCoS](https://arxiv.org/abs/2505.23060), binary rewards collapsed a small Llama model's training,
  and a pass-ratio reward beat binary at the second attempt (54.2 against 51.2) for Qwen2.5-1.5B.

## Credit for compiling

- **The failure moves, it does not vanish.** [CodeRL](https://arxiv.org/abs/2207.01780) graded a
  program by four outcomes — compile error, runtime error, failed test, passed. After training,
  compile errors became rarer and the share of runtime errors rose; over 90% of the compile errors
  left were syntax. RL loss alone hit vanishing gradients; it was mixed with cross-entropy after a
  supervised start.
- **Parse, then test, then resemble the reference.** [VeriReason](https://arxiv.org/abs/2505.11849)
  combines testbench correctness, successful parsing and AST similarity to a reference. Qwen2.5-1.5B
  went from 25.6 to 38.6 on VerilogEval-Machine pass@1 after fine-tuning and to 44.7 after GRPO, on
  1,892 problems; pass@5 moved only 46.3 to 49.1, so the RL gain was first-attempt accuracy.
- **A validator alone empties the meaning.** [SLMFix](https://arxiv.org/abs/2511.19422) weights its
  validator against AST similarity by the batch's validator pass rate, capped at 0.5. Rewarding the
  validator alone reached 98.99% validator pass while AST similarity to the reference fell from
  0.3455 to 0.2062; similarity alone reached 22.48%. And similarity is a poor judge of correctness:
  scored against tests, a 0.8 threshold caught only 38.4% of correct programs.
- **Gates rather than credit.** [TritonRL](https://arxiv.org/abs/2510.17891) makes its syntax and
  functionality checks necessary conditions for any correctness reward instead of adding credit for
  passing them.
- **No published ablation** compares credit from structured diagnostics with a test-only binary
  reward for a model under 2B; a term counting diagnostics is an experiment.

## Repair and localization: the fix is known

- **Similarity to the real patch.** [SWE-RL](https://arxiv.org/abs/2502.18449) scores −1 for a
  malformed answer and otherwise the `difflib` similarity, between 0 and 1, of the patch to the
  real one; it runs nothing. Llama-3.3-70B reached 41.0% on SWE-bench Verified — best of 500
  samples reranked by generated tests. An exact-match reward scored 29.0 against 34.8 for similarity
  on oracle-file repair. The −1 is as large as the whole positive range, and the authors list that
  similarity may block equivalent fixes.
- **F-score over locations.** [SoRFT](https://arxiv.org/abs/2502.20127) scores file, function and line
  localization by F-beta with beta 3, favouring recall, and gives 0 when the answer is empty or names
  any target absent from the problem — the whole answer, not that target. A hit reward, 1 if any true
  location appears, was gamed by listing more locations. Most of its gain came from rejection-sampled
  fine-tuning: 7.6 to 18.0 resolved on SWE-bench Verified at 7B, and 21.4 after RL.
- **Feedback in the context.** [RLEF](https://arxiv.org/abs/2410.02089) shows public-test results
  between turns and rewards passing all public and private tests, plus a small penalty for invalid
  code in non-final turns, dropped for single-turn training where the failure reward already covers
  it. Scoring on held-out tests is a guard: a model shown expected outputs can copy them.
- **Rewarding improvement invites sandbagging.** Rewarding only the second attempt's gain over the
  first led CoCoS's model to make its first attempt worse on purpose.

## Learned judges

DeepSeek-R1 applies no neural reward model to reasoning, citing reward hacking, and lists its process
reward model among unsuccessful attempts ([arXiv 2501.12948](https://arxiv.org/abs/2501.12948)).
AceCoder's learned reward lost to the rule from two starting models and won from one (51.5 against
50.9). Used as a dense reward, AceCodeRM-7B collapsed VeRPO's training after about 190 steps.
[PRIME](https://arxiv.org/abs/2502.01456)'s online implicit process reward added 4.1 points to RLOO
but 1.7 to GRPO, with LiveCodeBench falling from 25.8 to 23.9. A project with an exact compiler and
test runner has little to gain from them as the score.

## What it means for lotml

1. **Writing:** 1 when every grader-owned hidden test passes, read from `lotml test --json`, else 0,
   with `lotml check` errors scoring 0. A small credit for checking clean is an experiment to run only
   while almost nothing passes, kept well below a pass and switched off once it would reward "checks
   but wrong" — Agnostics' 0.2 for running cleanly lowered results. Credit for tests passed, if at
   all, only next to the binary term and weighted by difficulty, after a binary run plateaus.
2. **Repair:** the same grader, 0 when a test block changed — the guide tool's gate already refuses
   that fix (`specs/guide-tool/design.md`) — and similarity to the known fix only as a tie-breaker
   among unsolved samples.
3. **The harness guide** answers where to change and what kind of change, so its reward is
   SoRFT-shaped: F-beta over the declarations it names against the seeded ones, zero for a
   declaration absent from the file, and no reward for merely including the right one.
4. **Penalties for malformed output** stay small next to the success reward; the guide's answers are
   already constrained to a JSON schema (adr:0017-a-half-billion-coder-model-tuned-locally-and-served-by-llama-server).

## Not machine-checked

The reward constants in CodeRL's and RLEF's equations, and Arctic-Text2SQL-R1's syntax credit, sit in
formulas the converted text does not carry, so they are not quoted here. DeepCoder's reasoning for a
binary reward over partial credit is in its [blog](https://www.together.ai/blog/deepcoder), and TRL's
[environment guide](https://huggingface.co/docs/trl/openenv) reports binary rewards giving cleaner
signals than shaped ones.
