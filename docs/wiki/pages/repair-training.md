# Repair training

Training models to fix code and to say where to fix it — from a compile error to a repository issue.
lotml's harness guide is this kind of model: it names the declaration to change and the kind of
change, and the agent edits (adr:0017-a-half-billion-coder-model-tuned-locally-and-served-by-llama-server).
[[compiler-embedded-model]] surveyed small models inside a toolchain; this page adds how repair and
localization models are trained, with reinforcement learning ([[grpo]], [[verifiable-rewards]]) and
with distillation ([[small-coder-training]]), and what the bugs they train on should be. Every number
taken from a paper in `research/literature/sources.json` is quoted in `claims.json` there
([[source-verification]]).

## Repository agents: fine-tuning carries most of it

- **RL adds a few points at 8B.** [SWE-AGILE](https://arxiv.org/abs/2604.11716) took Qwen3-8B from
  15.83% on SWE-bench Verified to 21.45% with fine-tuning on 2.2k trajectories whose reasoning a larger
  model filled in, and to 24.05% with RL. Fine-tuning on the same trajectories with their original
  shallow reasoning scored 14.83%, below the untrained model, and RL on top reached 16.03%. It moved
  tool calls from JSON to XML because escaping code inside JSON made weak models write syntax errors.
- **Fine-tuning alone reached 42.2% at 8B** on about 18k teacher trajectories — 14.1k resolved plus
  about 4k that found the right file and failed the fix — with no loss on turns that drew an execution
  error and an easy-to-hard order, which together added 2.8 points ([SWE-Lego](https://arxiv.org/abs/2601.01426)).
  Untrained, Qwen2.5-Coder-7B resolved 1.0% and Qwen3-8B 7.6%.
- **Larger RL needs larger machines.** [SWE-RL](https://arxiv.org/abs/2502.18449) ran 512 H100s for
  about 32 hours; an agentic RL run of 25 steps took 8 nodes of 8 H100s for 50 hours, and GRPO did not
  beat fine-tuning on the hard subsets because too-hard problems give no advantage
  ([BugPilot](https://arxiv.org/abs/2510.19898)).
- **Guidance helps collect, not train.** [Agent-RLVR](https://arxiv.org/abs/2506.11425) took a 72B model
  from 9.4% to 22.4% with fine-tuning then offline DPO; re-attempting failures with a plan, a hint from
  the stack trace and the file location accounts for 19.8% to 22.4% of it, and rejection-sampled
  fine-tuning alone reached 20.8%, within one standard deviation. Hints left in the fine-tuning data
  lowered it to 16.8%.
- **Three 14B models, fine-tuned only,** resolve 46.00% choosing among 60 candidates
  ([Co-PatcheR](https://arxiv.org/abs/2505.18955)). Keeping only teacher samples with the right answer
  was necessary; asking the teacher to rationalize a known fix taught the student to invent
  identifiers; the localizer stopped improving between 2K and 5K issues; majority vote beat a learned
  outcome reward model (46.00 against 36.40).
- **A strong coder can be a poor agent.** Qwen2.5-Coder-32B hallucinated environment output and looped
  on formatting errors ([SWE-rebench](https://arxiv.org/abs/2505.20411)). The survey behind this page
  found no agentic RL result below 7B.

## Localization: small models that point

- **A 137M retriever.** [SweRank](https://arxiv.org/abs/2505.07849)'s smallest model puts every edited
  function in its top 10 for 74.45% of the 274 SWE-bench Lite issues that edit an existing function,
  above SWE-agent, Moatless and OpenHands with Claude 3.5 (64.60, 64.96, 70.07) and below LocAgent
  with Claude 3.5 (77.37). Qwen3-Embedding-0.6B fine-tuned the same way went from 62.77 to 75.18.
  Trained on Python only, it still lifted nine other languages (44.02 against 35.04), and mined
  training pairs left unfiltered made it worse.
- **Better pointing, modest repair.** With one editor, raising file accuracy from 69.7 to 83.2 raised
  repair from 21.0 to 24.5, and perfect localization only to 25.9: the editor is the bottleneck
  (SweRank). [LocAgent](https://arxiv.org/abs/2503.09089)'s 12% gain at Pass@10 is relative — 33.58 to
  37.59 with Claude 3.5 localizing and editing. Fine-tuned on 433 teacher and 335 self-generated
  successful trajectories, its 7B agent put every edited function in its top 10 for 71.53% against
  77.01% at 32B, for $0.05 a problem;
  removing its keyword search over a code index dropped the 7B to 53.28.
- **A fixed pipeline.** [Agentless](https://arxiv.org/abs/2407.01489) — localize, repair, validate —
  solved 32.00% of SWE-bench Lite at $0.70 per issue, with GPT-4o at every step; its generated
  reproduction tests were unreliable (213 reproduced the issue, 94 then confirmed the real fix), and
  sampling stopped helping at about 40 patches.

## Narrow repair behind a checker

- **The checker as critic.** Break-It-Fix-It reached 90.5% on GitHub-Python and 71.7% on DeepFix,
  the latter over up to 5 passes, and its learned breaker helps most when real broken examples are
  scarce ([BIFI](https://arxiv.org/abs/2106.06600)); [[compiler-embedded-model]] has the rest.
- **A 0.5B fixer behind a generator.** [SLMFix](https://arxiv.org/abs/2511.19422) trains Qwen2.5-Coder
  0.5B with RL to repair a larger model's output against a static validator: behind Qwen2.5-Coder 7B,
  96.92% Ansible validator pass against 59.56% for the 7B model and 73.83% for the 7B model
  fine-tuned. Trained to write Ansible directly, the same 0.5B model reached 48.83% against 94.30% as
  a fixer. Its experiments took about 100 GPU hours on two A40s.
- **Small models do not repair themselves.** Qwen2.5-1.5B left 93% of its wrong second attempts
  nearly identical to the first, against 32% for Gemini, and judged its own code's correctness at 42%
  accuracy ([CoCoS](https://arxiv.org/abs/2505.23060)); RLEF's instruct models resubmitted the same
  code despite feedback ([RLEF](https://arxiv.org/abs/2410.02089)). Self-repair is bottlenecked by the
  model's own feedback: Code Llama 13B's repairs passed 9.1% of the time with its own feedback, 20.1%
  with GPT-3.5's and 39.3% with GPT-4's ([Olausson et al.](https://arxiv.org/abs/2306.09896)) — feedback
  from a weaker model was never tested.
- **Type constraints help repair most.** Only 6% of compile errors in generated TypeScript were
  syntax; type-constrained decoding raised repair pass@1 by 37% relative, and for Gemma 2 2B from
  11.6 to 20.9 ([Mündler et al.](https://arxiv.org/abs/2504.09246)). Where a constraint leaves a model
  no way out it loops to the token limit. llama.cpp's grammar engine checks the whole vocabulary,
  so near-zero grammar overhead holds only where XGrammar is the engine
  ([XGrammar](https://arxiv.org/abs/2411.15100)); [[constrained-decoding]] has the lotml side.

## Bugs to train on

- **Seeded bugs train nearly as well as real ones.** With 1,000 tasks per strategy and one 7B student
  each, [SWE-smith](https://arxiv.org/abs/2504.21798) resolved 9.2% from reverted real fixes, 8.8% from
  functions a model rewrote, 8.6% from AST mutations and 5.7% from an open-ended "add a bug" prompt,
  which produced mostly trivial swaps. AST mutation costs nothing per bug. Training-task difficulty
  did not predict training value (12.4%, 10.8%, 13.6% and 12.2% across four levels), tasks solved
  again and again degraded the student and are capped at 3 trajectories, and a failing test in the
  prompt taught the student to skip reproducing the bug. Python-only training did not transfer to
  other languages, and its edits drifted toward Python syntax.
- **Bugs that arise from real work.** Bugs an agent introduced while adding a feature beat deliberate
  ones at 32B (51.93 against 49.87, no gain over the base mix for the deliberate ones); at 14B,
  fine-tuning on them lowered the score from 41.13 to 40.40 and only RL or the full mix helped
  (BugPilot).
- **Mutants first, real fixes after.** [RealiT](https://arxiv.org/abs/2207.00301) localized 30.92% of
  real single-token bugs trained on mutants alone, 16.07% on real fixes alone and 44.23% with both in
  that order. About 1,000 real fixes already helped, and 3,314 matched the gain of scaling mutants
  from 5 to 100 per snippet. Fine-tuning on real fixes lowered its score on mutants while raising it on
  real bugs: a seeded benchmark does not rank models for real ones.
- **Keep only bugs that break a test the fix repairs.** Qwen3-Coder-Next kept a synthetic bug only if
  it failed existing tests and reverting its patch fixed them, and hid the bug-triggering tests from
  the agent ([arXiv 2603.00729](https://arxiv.org/abs/2603.00729)); real pull requests gave SWE-Lego
  most of its gain, synthetic bugs adding more as they scaled. DeepSeek-R1's code RL mixed 8k
  bug-fixing problems from real issues into its algorithm problems
  ([arXiv 2501.12948](https://arxiv.org/abs/2501.12948)).

## What it means for lotml

1. **The harness guide is a localizer, and localizers train well by fine-tuning.** SweRank's 137M
   retriever, LocAgent's 7B and Co-PatcheR's localizer were fine-tuned only; RL added 3.4 points over
   rejection-sampled fine-tuning in SoRFT ([[verifiable-rewards]]) and 2.6 in SWE-AGILE. RL for the
   guide comes after it stops improving on harder data, and only on tasks it sometimes gets right.
2. **Seeded failures are good training data and a poor test.** `lotml dev mutate` is SWE-smith's
   procedural strategy, which trained within error bars of real fixes; RealiT shows the seeded score
   does not rank models on real bugs. Held-out real failures decide, as `specs/guide-evaluation/`
   already requires.
3. **Harder seeded tasks:** several mutations in one program, mutations that check clean and fail a
   test, multi-file programs — each kept only if a hidden test fails and the known fix passes.
4. **Real failures, as they come.** The trace dataset (`specs/trace-dataset/`) collects repairs from
   agent runs; RealiT's thousand real fixes is the order of magnitude where they started to help.
5. **Feedback from a separate model is the untested case.** Olausson et al. only tested stronger
   feedback; the guide is a weaker model advising a stronger agent, which only agent runs with and
   without it can measure.
6. **Its answers are JSON under a schema,** and SWE-AGILE's weak models failed on code escaped inside
   JSON; the tuned guide's answers all came back valid in the pilot (adr:0017), which constrained
   decoding guarantees for the schema but not for the code inside it.

## Not machine-checked

DeepDelta's repair of Java build errors from compiler diagnostics
([Google Research](https://research.google/pubs/deepdelta-learning-to-repair-compilation-errors/));
DeepSWE's RL of a 32B agent ([blog](https://together.ai/blog/deepswe)); SkyRL's
([report](https://novasky-ai.notion.site/skyrl-v0)).
