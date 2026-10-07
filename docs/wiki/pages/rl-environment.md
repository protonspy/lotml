# RL environment

Everything around a policy while reinforcement learning trains it: the tasks, the tests that judge
them, the grader that runs the tests, the sandbox it runs in, and the evaluation that says whether
anything was learned. The reward's shape is [[verifiable-rewards]]; this page is about the program
that computes it, which decides what is learned at least as much. Every number taken from a paper in
`research/literature/sources.json` is quoted in `claims.json` there ([[source-verification]]).

## Weak tests teach wrong code

- **How weak, measured.** On AtCoder problems, judging programs from Qwen2.5-Coder-7B-Instruct,
  TACO's tests reached 69.97 precision and 72.53 recall; regenerated ones 85.64 and 94.77. On the
  hardest problems precision was 21.67 against 60.00 ([HardTests](https://arxiv.org/abs/2505.24098)).
  Tests made only of small inputs an LLM writes reached 55.48 precision: wrong programs that are
  merely slow pass them, and 30.0% of incorrect LLM programs fail by time limit against 14.9% of
  human ones. Inputs from generator programs aimed at likely-wrong solutions close part of the gap.
- **What weak tests do to RL.** GRPO on Qwen3-4B with the same problems and different tests: pass@1
  went from 38.48 to 36.95 with TACO's and to 39.42 with HardTests', while pass@10 rose under both — on
  105 problems, where the drop is under two problems. In [AceReason-Nemotron](https://arxiv.org/abs/2505.16400)
  both false positives (wrong code passing) and false negatives (wrong tests failing right code) led
  to early convergence on poor policies or to collapse.
- **Validated tests are still imperfect.** After filtering, a human audit found 3 of 200 tests invalid
  ([AceCoder](https://arxiv.org/abs/2502.01718)), whose authors concede that passing does not prove
  correctness. Visible example tests let wrong programs through ([CodeRL](https://arxiv.org/abs/2207.01780)).
- **The verifier's own bugs.** Two configurations of one math verifier disagreed on 49.9% of answer
  pairs known to be equivalent, whitespace and punctuation causing 93.0% of one configuration's
  failures; a verifier that crashes has to be counted apart from one that rejects
  ([verifier audit](https://arxiv.org/abs/2609.01354)). It is math and runs no training, but it is the
  same class of bug as a test harness comparing printed output.

How the recipes validated their tests:

- **Against a reference.** [Skywork-OR1](https://arxiv.org/abs/2505.22312) kept a problem only if its
  reference solution passed every test. AceCoder removed every test one stronger model's program
  failed and dropped questions left with fewer than 5.
- **By agreement.** [rStar-Coder](https://arxiv.org/abs/2505.21297) runs 16 solutions on at least 50
  inputs and keeps outputs a majority agree on (60%, 40% for hard problems); on 64 seed problems with
  known solutions that labelled 96.8% of outputs correctly, against 12.7% for GPT-4o writing outputs
  directly. [DeepSeek-R1](https://arxiv.org/abs/2501.12948) had a model write test generators and kept
  tests that passed correct submissions and caught incorrect ones.
- **By coverage and count.** [KodCode](https://arxiv.org/abs/2503.02951) keeps a solution with its
  tests only at 100% branch coverage; [Klear-Reasoner](https://arxiv.org/abs/2508.07629) dropped code
  problems with fewer than 16 tests; [MultiPL-T](https://arxiv.org/abs/2308.09895) generated five
  test suites per function and discarded failing tests.

## Task pools calibrated to the policy

- **Drop what is always or never solved.** DAPO's dynamic sampling drops groups that are all right or
  all wrong during training ([arXiv 2503.14476](https://arxiv.org/abs/2503.14476)).
  [Seed-Coder](https://arxiv.org/abs/2506.03524) also dropped problems solved over 87.5% of the time;
  [MiMo](https://arxiv.org/abs/2505.07608) dropped math problems its model solved over 90% of 16 times
  but code only at 16 of 16. [VeriReason](https://arxiv.org/abs/2505.11849) keeps problems whose mean
  reward lies between 0.3 and 1.8 with spread above 0.1. [Arctic-Text2SQL-R1](https://arxiv.org/abs/2505.20315)
  kept synthetic tasks only when one of ten samples from the trained model was correct; unfiltered
  ones lowered the result. AceCoder kept the hardest 25%.
- **Retiring solved problems: two results.** [ScaleRL](https://arxiv.org/abs/2510.13786) retires a
  prompt for good once its pass rate reaches 0.9, which improved scaling; MiMo found removing solved
  problems destabilized training and keeps them in an easy pool sampled 10% of the time.
- **A pass-rate filter does not check the task.** Ill-posed problems passed Skywork-OR1's difficulty
  filter because the model sometimes produced the expected answer.
- **Small pools worked.** RL sets of 1,892 problems (VeriReason), 3,995 query-verifier pairs
  ([Qwen3](https://arxiv.org/abs/2505.09388)), 5,369 ([Agnostics](https://arxiv.org/abs/2508.04865))
  and 8,520 (AceReason); 700 generated arithmetic problems were enough for steady gains
  ([arXiv 2507.10532](https://arxiv.org/abs/2507.10532)). Easy problems transfer: RL on simpler math
  questions gave the best results on harder benchmarks ([Dr. GRPO](https://arxiv.org/abs/2503.20783)),
  and RL on single-kernel tasks alone transferred best in [TritonRL](https://arxiv.org/abs/2510.17891).
- **A sanity set first.** Problems the starting model solves between 20% and 80% of the time — 1,460
  for a distilled 1.5B model — make a set RL should be able to perfect, which tests the setup before a
  real run ([FP16](https://arxiv.org/abs/2510.26788)).
- **Contamination reaches code.** Given 80% of a LiveCodeBench problem, Qwen2.5-7B reproduced the rest
  exactly 85.71% of the time and Llama-3.1-8B 4.40% (arXiv 2507.10532): a Qwen model may recall a
  translated Python benchmark too. KodCode flags overlap above 0.95 embedding similarity; most of its 94
  flags of 447K questions were HumanEval- and MBPP-level problems.

## The grader and the sandbox

- **Code is slow to verify.** With 64 workers, AceReason verified 1,024 math answers in about 3.9 s
  and 1,024 programs in about 552.4 s, some 142 times slower.
- **Timeouts and limits.** [The pass-rate study](https://arxiv.org/abs/2605.02944) set each problem's
  timeout to three times the reference solution's runtime, between 10 and 30 s, in firejail;
  [ProRL](https://arxiv.org/abs/2505.24864) scored zero past 5 seconds in total. Agnostics reads
  output through a 5 MB buffer and kills what overflows it, because a bad program can print
  gigabytes, and keeps compile and run timeouts apart.
- **Flaky tests.** [SWE-rebench](https://arxiv.org/abs/2505.20411) runs tests several times and drops
  any task that fails once.
- **What lotml already has.** The agent harness appends hidden tests named with a `hidden:` prefix
  (`specs/agent-harness/design.md`); the guide tool's gate refuses a fix that changes a test block and
  runs candidates in a fresh scratch directory, where lotml without an interface reaches no module but
  `math` and no I/O beyond `print` (`specs/guide-tool/design.md`). On Windows only the deadline bounds
  a candidate's memory, which is one reason for a Linux grader; fast generation is the other
  ([[small-coder-training]]).

## Evaluation that can tell learning from noise

- **Pass@k against the starting model.** Narrowing shows only there: between steps 150 and 450 of a
  GRPO run of Qwen2.5-7B, pass@1 on the training split rose from 26.1 to 42.5 while pass@256 fell from
  66.3 to 64.3, below the base's 67.2 ([Yue et al.](https://arxiv.org/abs/2504.13837)); VeriReason's RL gain was 6.1 points at
  pass@1 and 2.8 at pass@5.
- **A random-reward control, run like the real one.** Random rewards gained 21.4 MATH-500 points on
  Qwen2.5-Math-7B against 29.1 for correct ones, through GRPO's clipping amplifying what the model
  already does; it often failed on Llama and OLMo, and models already RL-tuned barely moved under any reward
  ([Spurious Rewards](https://arxiv.org/abs/2506.10947)). The signal exists only through clipping —
  with one update per batch of samples random rewards do nothing — so the control must use the main
  run's updates per batch. On problems newer than the model, only the correct reward helped (arXiv
  2507.10532).
- **Template and format.** The official chat template cut Qwen2.5-Math-7B's greedy MATH-500 from 72.20
  to 50.60 (arXiv 2507.10532), and a prompt alone moved it by 19.4 points (Spurious Rewards): a gain can
  be recovery from a format the model was handed.
- **Real failures held out.** Mutant benchmarks did not track real bugs in
  [RealiT](https://arxiv.org/abs/2207.00301), and [SWE-smith](https://arxiv.org/abs/2504.21798)'s
  authors call their seeded tasks unfit for evaluation. A time split catches contamination: one model
  resolved 39.7% of SWE-bench Verified and 21.3% of fresh tasks (SWE-rebench), which reports the mean
  of five runs with its standard error.
- **Impossible tasks.** Tests mutated to contradict the specification score cheating directly:
  any pass is a hack ([[reward-hacking]]).
- **Sample sizes.** AMC23 is 40 problems, 2.5 points each ([Open-RS](https://arxiv.org/abs/2503.16219));
  HardTests' 105 problems are about 0.95 points each. The paired tests and sample sizes lotml uses are
  in [[evaluation-harness]].

## What it means for lotml

1. **The grader is the product.** `lotml test --json` reports each test's outcome — pass, fail, error,
   panic — and the grader reads that report, counts the hidden tests that ran against those expected,
   and never trusts an exit code.
2. **Measure test strength with the compiler's own mutants.** A hidden suite is worth what it rejects:
   running mutants of the reference through it (`lotml dev mutate`) gives a kill rate per task, the
   cheap analogue of HardTests' precision. Tasks whose suites let mutants pass are dropped or get more
   tests.
3. **Calibrate the pool on the model being trained:** sample it, keep tasks it solves sometimes, and
   keep the always-solved ones as a small easy pool rather than deleting them outright.
4. **Hold out by problem and by source.** The corpus is HumanEval and MBPP translated, which a Qwen
   model may partly recall; the problem split keeps them apart, and real failures harvested from agent
   runs stay out of training.
5. **Every run gets a random-reward twin and a pass@k curve,** and impossible tasks sit in the
   evaluation set.

## Not machine-checked

DeepCoder's sandbox and test requirements ([blog](https://www.together.ai/blog/deepcoder)),
[Code-R1](https://github.com/ganler/code-r1)'s firejail setup and its warning that a sandbox missing
libraries creates false positives, [SandboxFusion](https://github.com/bytedance/SandboxFusion),
DeepSWE's move from Docker to Kubernetes ([blog](https://together.ai/blog/deepswe)), and environment
interfaces meant for sharing — Prime Intellect's [verifiers](https://github.com/PrimeIntellect-ai/verifiers),
[OpenEnv](https://huggingface.co/blog/openenv) and TRL's [reward functions](https://huggingface.co/docs/trl/main/en/grpo_trainer).
