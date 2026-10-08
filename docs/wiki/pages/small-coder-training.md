# Small-coder training

How a model of 0.5B to 1.5B parameters learns to write and repair a programming language:
distillation from a stronger model, then reinforcement learning against a verifier. lotml's harness
guide is trained by supervised fine-tuning alone today
(adr:0017-a-half-billion-coder-model-tuned-locally-and-served-by-llama-server). This page is the
entry to what the evidence says about doing more, and to the pages that go deeper: the algorithm
([[grpo]]), the reward ([[verifiable-rewards]]), what surrounds the policy while it trains
([[rl-environment]]), how training games the grader ([[reward-hacking]]) and models that fix code
([[repair-training]]). Every number taken from a paper in `research/literature/sources.json` is
quoted in `claims.json` there and checked against the paper ([[source-verification]]); blogs,
documentation and model cards are cited by link, without numbers.

## Distillation teaches; reinforcement learning sharpens

- **At 32B, distillation beat RL from the base.** In [DeepSeek-R1](https://arxiv.org/abs/2501.12948),
  Qwen2.5-32B trained by RL from the base on math, code and STEM for over 10K steps scored 40.2 on
  LiveCodeBench pass@1; the same base fine-tuned on DeepSeek-R1's samples scored 57.2. The distilled
  models are supervised fine-tuning on 800,000 samples, with no RL stage. The authors conclude that
  smaller models relying on large-scale RL "require enormous computational power and may not even
  achieve the performance of distillation"; pure RL from a 7B dense and a 16B MoE base fell into
  repetition, though Qwen2-Math-7B did improve under pure RL.
- **At 8B, distillation won at a tenth of the compute.** From one distilled Qwen3-8B checkpoint at
  42.0 on LiveCodeBench v5, RL reached 52.9 in 17,920 GPU hours and on-policy distillation 60.3 in
  1,800 ([Qwen3](https://arxiv.org/abs/2505.09388)); on AIME, RL raised pass@1 but not pass@64, and
  distillation raised both. Every dense Qwen3 model from 0.6B to 14B was built by distillation from larger ones.
- **RL still adds on top of a distilled model.** [AceReason-Nemotron](https://arxiv.org/abs/2505.16400)
  took a distilled 7B model from 37.6 to 44.4 on LiveCodeBench v5 with math-only RL and to 51.8 after
  code RL — yet at 7B that only matched a distillation-only model on code, and the authors put RL's
  clearer edge at 14B and above. In [MiMo](https://arxiv.org/abs/2505.07608), from one 7B base, RL
  alone reached 49.1 on LiveCodeBench v5 (from 32.9), supervised fine-tuning alone 52.3 and both in
  order 57.8. [Seed-Coder](https://arxiv.org/abs/2506.03524) gained 14.6 points from RL after a small
  long-reasoning warm-up, started from the base model because the instruct model collapsed into its
  fine-tuning patterns during RL.
- **Weak starts on hard problems mostly failed.** RL against an exact verifier
  ([Agnostics](https://arxiv.org/abs/2508.04865)) did not improve Qwen3-1.7B or Llama-3.2-3B on
  competition problems, which the authors put down to difficulty; SmolLM3-3B, starting at 1–2%,
  did improve (Lua to 8%); the paper shows no reward curves for the runs that failed. Fortran and
  OCaml gave zero reward for the first several steps. The
  [Dr. GRPO](https://arxiv.org/abs/2503.20783) authors state the rule: a model that cannot sample a
  single correct answer gets no reward to learn from.
- **RL concentrates what the model already samples.** RL-trained models beat their starting models
  at small k and are overtaken as k grows ([Yue et al.](https://arxiv.org/abs/2504.13837)); the GRPO
  paper itself saw majority voting improve and pass@k not
  ([DeepSeekMath](https://arxiv.org/abs/2402.03300)). [ProRL](https://arxiv.org/abs/2505.24864) finds
  the narrowing task-dependent, with code among the tasks that kept gaining, after about 16k GPU
  hours. Either way a short run is judged at pass@k as well as pass@1 ([[rl-environment]]).
- **Small runs are cheap and noisy.** [Open-RS](https://arxiv.org/abs/2503.16219) put 24 hours on
  4×A40 at $42; AMC23 rose from 63% to 80% within 50–100 steps and declined after 150–200, with
  output drifting into other languages. Re-evaluated under one protocol by
  [Tina](https://arxiv.org/abs/2504.15777), the published checkpoint scored 62.50 on AMC23, the same
  as its base — and AMC23 is 40 problems, 2.5 points each. Tina's LoRA run cost $9 to its best
  checkpoint, and two checkpoints 50 steps apart averaged 50.60 and 43.05. Both are math, at 1.5B,
  from a distilled model.

## What the teacher exposes decides the method

- **Text.** Fine-tuning on teacher outputs built DeepSeek-R1's distilled models,
  [OpenCodeReasoning](https://arxiv.org/abs/2504.01943) (supervised only, 61.8% on LiveCodeBench at
  its best) and [rStar-Coder](https://arxiv.org/abs/2505.21297), whose 1.5B model reaches 88.4 on
  HumanEval against 70.7 for its base and 40.1 on LiveCodeBench against 6.5 — trained on long
  reasoning and decoded with up to 32k output tokens.
- **Every token's probability.** In on-policy distillation the student samples and the teacher
  scores each of its tokens; it is what Qwen3 used, and it needs the teacher's logits and a shared
  tokenizer. Caching only the teacher's top-K probabilities is biased: below 25 tokens a 300M
  student did worse than plain cross-entropy training, and handing the leftover probability to the reference
  token recovered 73% of the gap at K = 20 ([Sparse Logit Sampling](https://arxiv.org/abs/2503.16870),
  pre-training distillation). An API returns a short list of top log-probabilities at most, so an API
  budget buys text.
- **A teacher that does not know the data.** Distilling from a teacher not adapted to the student's
  data gave only a small gain over plain fine-tuning (Sparse Logit Sampling): adapt the teacher first.
- **Size and length.** A 0.5B student averaged 20.4 when taught by a 3B model and 16.9 when taught by
  a 72B one, and prompting the large teacher to be simpler did not close it
  ([Li et al.](https://arxiv.org/abs/2502.12143)). On MATH, long reasoning trailed short by 8.44
  points at 0.5B, 10.7 at 1.5B and 4.84 at 3B; mixing one long example to four short lifted the 3B
  student to 64.7 against 56.2 and 61.0. That study is math only. For code, rStar-Coder's 1.5B gained
  from long traces, and stripping self-reflection from OpenThoughts' traces cut the student's
  average by 49.1% relative ([OpenThoughts](https://arxiv.org/abs/2506.04178)).
- **The stronger model is not always the better teacher.** QwQ-32B's traces beat DeepSeek-R1's by
  1.9 points on code questions though QwQ-32B scores lower (OpenThoughts); with the same prompts,
  three teachers gave students from 44.44 to 53.05 on LiveCodeBench
  ([Klear-Reasoner](https://arxiv.org/abs/2508.07629)). The teacher is something to try, not assume.
- **Filtering teacher output, with a fluent teacher.** OpenThoughts' "no filter beat no filtering"
  compared 31,600 filtered samples with 63,200 unfiltered; at equal size on code, judged filters
  slightly beat random ones (41.3 and 40.7 against 39.8). Fine-tuning on 151,251 solutions labelled
  incorrect scored 52.3 against 47.0 for as many labelled correct, which the authors put down to the
  incorrect ones covering harder questions (OpenCodeReasoning). Verification helped OpenThoughts' 32B student and hurt its 7B one. Where the
  model wrote the data itself, execution mattered: responses passing their tests scored 65.2,
  random ones 61.6, failing ones 57.9 ([SelfCodeAlign](https://arxiv.org/abs/2410.24198)).
- **More problems beat more answers per problem.** On rStar-Coder's 37.7K seed problems, going
  from 1 to 16 solutions each took a 7B model from 40.8 to 54.7 on LiveCodeBench (603K examples);
  the full set, 480K distinct problems in 580K examples, scored 57.3 with fewer examples — "expanding
  problem diversity is more effective and efficient than only increasing the number of solutions
  per problem".

## A language no teacher knows

[[training-prior]] has the size of the problem; these are the training results.

- **Translate tested programs, keep what passes.** [MultiPL-T](https://arxiv.org/abs/2308.09895)
  translates Python functions with their tests: Racket at 1B scored 11.3 with validated data, 8.6
  without validation and 4.7 untuned; at 15B, 21.0, 11.6 and 11.8. Its datasets hold 37,592 to
  48,194 items per language; deduplication cut Lua from 1.4M to 48K items with no loss (17.3 after
  under 30 minutes against 17.1 after 12 hours), and the tuned 1B model matched the untuned 15B on
  Racket and OCaml. Fine-tuning on the Python source itself changed nothing (15.1 to 15.0): the gain
  is new data, not more Python.
- **The verifier against the teacher.** Fine-tuning Qwen3-4B on 1,987 Fortran solutions from Claude
  Sonnet 4 — a teacher that itself scored 12%, in a $96 set used unfiltered — took it from 0% to 3%;
  RL against an exact input/output verifier reached 15% (Agnostics). The authors blame the weak
  teacher; the experiment does not compare verified with unverified teacher data. Fortran is 0.07%
  of The Stack v2: rare, not absent.
- **Python as a bridge in the teacher's prompt.** Giving the data generator a Python solution added
  5.73–11.55 points on M-HumanEval over fine-tuning on directly generated data
  ([Bridge-Coder](https://arxiv.org/abs/2410.18957)), and Python was the best bridge language.
  Training first with the bridge in the student's input and then without it beat either alone.
  This is the teacher's prompt: at inference, a Python solution in GPT-5's prompt cut Cangjie pass@1
  ([[training-prior]]). Python-heavy instruction tuning also left one base model worse at D and Bash.
- **A small student needs a stronger teacher.** SelfCodeAlign's own data beat GPT-4o's at equal size
  on the same instructions (67.1 against 65.9 on a 7B base), but StarCoder2-3B scored 35.4 on its
  own data and 42.1 on DeepSeek-Coder-33B's.
- **Pretraining then pairs.** For Mojo ([MojoBench](https://arxiv.org/abs/2410.17736)), 6.58M tokens of
  continued pretraining took CodeGemma-7B from 5.1% to 36.7% on HumanEval-Mojo and 3,200 fine-tuning
  pairs — 800 solutions, four prompts each — to 66.4%; fine-tuning alone reached 42.3. A code base
  model mattered: Mistral ended at 26.9. Untrained models got the algorithm right and the syntax
  wrong.
- **Syntax is learned fast; fidelity is not.** In [PyLang](https://arxiv.org/abs/2605.15607), 2,250
  examples took Qwen3 4B, 8B and 32B from syntax errors on 100%, 94.6% and 77.2% of problems to 4.5%,
  4.2% and 2.5%, and problem pass to 36.1%, 38.6% and 47.2%. Halving the data cost Qwen3-8B 1.8
  points (36.6 against 38.4); GRPO on top added 2.9 to the new language and 2.8 to Python. Of the problems a
  fine-tuned model solved in Python and failed in PyLang, 27–51% used the right algorithm wrongly and
  30–58% a different one.
- **Quantized models drift, not only toward Python.** Quantization amplifies language confusion; the
  quantized Llama-3.1-8B drifted to C++ in 89.55% of migrations, and the authors ask for the
  reduced-size model itself to be tested ([Moumoula et al.](https://arxiv.org/abs/2503.13620)).

## What it means for lotml

1. **The harness guide's validation split cannot drive RL.** The tuned guide puts the right
   declaration first on all 200 seeded validation records sampled
   (adr:0017-a-half-billion-coder-model-tuned-locally-and-served-by-llama-server): every group of samples would
   score alike, and such a group carries no gradient ([[grpo]]). RL for the guide needs harder seeded
   tasks or real failures ([[repair-training]]).
2. **Order: verified data, fine-tuning, then RL on what the model sometimes solves.** The
   small-model RL successes above started from distilled or fine-tuned checkpoints, except
   SmolLM3-3B's climb from 1–2% to 8%; a model has to sample some correct answers before RL has
   anything to reinforce. RL is a polishing stage of a few points, judged at pass@k and against a
   random-reward control ([[rl-environment]]).
3. **The seed has to be rebuilt before it can be trained on.** The phase 2 corpus, 509 HumanEval
   and MBPP tasks translated by rules and by a model with the compiler, each kept only with its
   passing tests (`harness/results/corpus.md`), translates MultiPL-E's typed copies, whose licence
   forbids training (`harness/lotml_harness/agent/licences.toml`, `harness/results/NOTICE.md`). The
   same recipe run on openai/human-eval (MIT) and MBPP's original release (CC BY 4.0) gives a seed
   that can be trained on — MultiPL-T's recipe at a hundredth of its size. In PyLang, going from half to all of 2,250
   examples still added 4.3 points at 4B and 1.8 at 8B, and nothing larger was tried; MultiPL-T's
   sets hold 37,592 to 48,194 items. Growth comes first from more distinct Python sources, then
   from more solutions per problem, with deduplication, every item checked by `lotml check` and
   `lotml test`.
4. **The teacher has to learn lotml first.** No teacher knows it, an API gives text, and in the one
   study of it a top-K of log-probabilities short of 25 tokens did worse than none — in
   pre-training, at 300M. Token-level distillation would need a
   local open teacher sharing the Qwen2.5-Coder tokenizer the records are counted with (adr:0017),
   fine-tuned on lotml before it teaches; a smaller teacher may teach a 0.5B student better.
5. **Writing needs size; repairing does not.** The RL results on writing start at 1.5B from a
   distilled model; at 0.5B the evidence is repair behind a checker ([[repair-training]]).
6. **Measure the shipped artifact.** The guide ships quantized to Q4_K_M; drift toward Python, or
   any other language, is measured on that file.
7. **RL at this size needs a non-zero start, built on purpose.** The Prolog, Q, Lean and Verilog
   numbers in this item come from `research/llm-landscape/evaluation-and-adaptation.md`, not from
   `sources.json`: they are not machine-checked ([[source-verification]]); Agnostics' is.
   Qwen2.5-Coder-0.5B under GRPO
   learned nothing on Prolog zero-shot (0.00), and reached 0.13 with one example in the prompt as a
   syntax anchor ([arXiv 2506.11027](https://arxiv.org/abs/2506.11027)). On Q, a language the base
   models scored 0.0% on from 1.5B to 7B, GRPO at 1.5B made pass rates decline, and solutions and
   tests written by one model were gamed ([arXiv 2508.06813](https://arxiv.org/abs/2508.06813)).
   Agnostics needed only 0.09% base accuracy at 4B to start learning. The order this asks for: SFT
   first; a curriculum that has RL complete only the tail of a reference program at first
   (StepCoder); prompts filtered to a pass rate strictly between 0 and 1; a one-shot anchor while
   the pass rate is near zero. Expect a few points over SFT, not new ability: Lean +1.2 after
   +20.7 from SFT, Verilog +4 to +11 after +34. Sources:
   `research/llm-landscape/evaluation-and-adaptation.md`.

## Not machine-checked

Cited for what they describe, without numbers: DeepCoder's RL of a distilled 1.5B coder
([model card](https://huggingface.co/agentica-org/DeepCoder-1.5B-Preview),
[blog](https://www.together.ai/blog/deepcoder)); TinyZero's report that a 0.5B base does not learn to
reason under RL ([repository](https://github.com/Jiayi-Pan/TinyZero)); a GRPO run of
Qwen2.5-Coder-1.5B on Rust with cargo's build, lint and tests as rewards
([Oxen.ai](https://ghost.oxen.ai/training-a-rust-1-5b-coder-lm-with-reinforcement-learning-grpo/));
on-policy distillation and LoRA for RL as Thinking Machines measured them
([distillation](https://thinkingmachines.ai/blog/on-policy-distillation/),
[LoRA](https://thinkingmachines.ai/blog/lora/)); TRL's
[GKD trainer](https://huggingface.co/docs/trl/main/en/gkd_trainer); OpenRouter's
[log-probability parameters](https://openrouter.ai/docs/api-reference/parameters); and vLLM's
absence from native Windows, which puts GRPO with fast generation under WSL2
([Unsloth](https://unsloth.ai/docs/get-started/install/windows-installation)).
