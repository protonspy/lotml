# Evaluation harnesses and teaching a model a new language — prior art for LotML

Research of 2026-10-07. Topic A: harnesses that evaluate code generation, and the statistics behind
them. Topic B: measured ways to teach a model a language it never saw. Every number is checked
against its primary source: the paper (arXiv abstract, HTML or the converted copy in
`research/literature/cache/`), the repository's README or LICENSE, or official documentation.
Web pages were read through an extraction step that can paraphrase, so a quoted phrase is exact
only where it comes from the local cache. **UNVERIFIED** marks what could not be checked, and why.
Nothing was cloned or run.

## Summary

1. Contamination survives translation: 18.9% of HumanEval and 20.8% of MBPP solutions appear in The Stack, and refactoring did not reliably lower scores — a translated HumanEval/MBPP measures fidelity to LotML, not problem solving.
2. EvalPlus found 18 defective canonical solutions in HumanEval (11%); under adr:0015 the hidden tests are those solutions' own outputs, so those tasks grade against wrong answers.
3. The agent harness's Wilson interval pools runs rather than tasks, which understates the error once a task has several runs; Miller and Inspect cluster by question, and a per-task fraction needs a t-interval over tasks.
4. At 198 tasks and one sample each, the smallest detectable difference is about 13 points (Miller, under his assumptions) and ten samples bring it to 7.5: a "not significantly worse" gate at about 200 pairs is not evidence of equivalence.
5. Agentic harnesses (SWE-bench, Terminal-Bench/Harbor, Inspect, SandboxFusion, MultiPL-E's container) assume Docker on Linux; on Windows the native options are Job Objects (already used), AppContainer and Windows Sandbox.
6. In context, examples beat rules: removing a grammar book's explanations barely hurt translation, removing its parallel sentences did; grammar prompting helped only DSLs absent from pretraining.
7. The closest analogue, the Q full-stack report: base Qwen-2.5 1.5B–7B scored 0.0%, the full stack reached 59%, GRPO on the 1.5B made pass rates decline, and jointly generated solutions and tests were gamed.
8. At 0.5B, GRPO did not learn Prolog zero-shot (0.00) but reached 0.13 with one in-context example; Agnostics needed only 0.09% base accuracy to start learning.
9. RL on top of SFT adds a few points — DeepSeek-Prover-V1.5 (Lean) SFT +20.7 then RL +1.2 at pass@128; CodeV-R1 (Verilog) SFT +34 then RL +4 to +11 — and compile-only rewards raise compile rate, not correctness.
10. MultiPL-T's per-language sets hold 37.6K–48.2K test-validated items, 74–95 times the 509-task seed, and that seed is itself untrainable: it derives from MultiPL-E, whose licence forbids training.

## What is new versus the wiki pages

The wiki pages (`evaluation-harness`, `training-prior`, `small-coder-training`, `rl-environment`,
`verifiable-rewards`, `python-leakage-pilot`) already carry MultiPL-E's popularity and doctest
findings, MultiPL-T's headline numbers, SPEAC, Agnostics' 4B results, RLEF's reward design, CodeRL,
CangjieBench, PyLang, EsoLang-Bench, MojoBench, Bridge-Coder, SelfCodeAlign, the McNemar sample-size
table and the RL-reward literature. This study adds:

- **Benchmarks:** the mechanics, flaws and licences of HumanEval-X, McEval, BigCodeBench, CRUXEval,
  EvalPlus, LiveCodeBench (versions, errata), SWE-bench with its Verified, Multilingual and Multi-
  variants and their audits, Aider polyglot and Terminal-Bench 2.0.
- **Frameworks and sandboxes:** bigcode-evaluation-harness, lm-evaluation-harness and Inspect AI;
  isolation tools on Linux and Windows, with licences.
- **Contamination:** Riddell et al. and Cao et al., which bear on translating HumanEval.
- **Statistics:** Chen's estimator in stable form, Miller's clustered and paired errors and his
  minimum-detectable-effect formula, Bowyer et al. on intervals at small N, seed and hardware
  variance (Madaan, Hochlehnert), pass^k and G-Pass@k, AI2's signal-to-noise ratio.
- **In-context learning:** rules versus examples (grammar prompting, MTOB, Aycock et al.),
  DocPrompting, MoonBit's design claims.
- **Data and transfer:** IRCoder, cross-language transfer predictors, the low-resource survey,
  Qwen2.5-Coder's small-model baselines, MultiPL-T's pipeline yields and licences, the Q report,
  AlphaVerus.
- **Compiler-feedback RL:** StepCoder, RLTF, PPOCoder, COMPCODER, RLCF, DeepSeek-Prover-V1.5,
  CodeV-R1, Prolog GRPO at 0.5B, Skopin and Kotelnikov at 0.6B, MURPHY, μCode, RLEF's 8B budget,
  Agnostics' training configuration and licence.
- **Two findings in this repository:** the pooled Wilson interval
  (`harness/lotml_harness/agent/report.py:24-32`, used at line 59), and `small-coder-training.md`
  calling the phase 2 corpus "the seed" while `harness/lotml_harness/agent/licences.toml` and
  `harness/results/NOTICE.md` forbid training on anything derived from MultiPL-E.

## Topic A — evaluation harnesses

### A.1 Function-level benchmarks

**MultiPL-E** ([arXiv 2208.08227](https://arxiv.org/abs/2208.08227), local cache;
[repo](https://github.com/nuprl/MultiPL-E)) translates HumanEval and MBPP by compiling four parts:
the signature and types (inferred from MBPP's assertions where Python had none), Python terminology
in the docstring ("list" becomes vector or array), the tests (a recursive value compiler, first-order
values only), and per-language stop tokens. It excludes 3 tasks with helper functions and up to 5
with untranslatable types. Beyond the wiki: "the presence of the doctests is important, though their
translation is not" (no translation versus test-only translation, p = 0.2); Lua ranked 9th with 0.2%
of GitHub activity; "pass@1 rates appear to stabilize around 20 samples". Execution runs in a
container started with `podman run --rm --network none`
([README](https://raw.githubusercontent.com/nuprl/MultiPL-E/main/README.md)). Licence: the GitHub
LICENSE is "BSD 3-Clause License with Machine Learning Restriction", whose clause 4 says the contents
"may not be used as training data for any machine learning model"; the
[Hugging Face card](https://huggingface.co/datasets/nuprl/MultiPL-E) declares `mit`. The project
follows the stricter one. The language count disagrees across sources: 18 targets in the paper and
README, "22" in the card's prose, 24 `humaneval-*` configurations in its YAML.

**HumanEval-X** ([arXiv 2303.17568](https://arxiv.org/abs/2303.17568);
[repo](https://github.com/zai-org/CodeGeeX), Apache-2.0; dataset Apache-2.0) rewrites HumanEval by
hand in C++, Java, JavaScript and Go: "820 hand-written problem-solution pairs (164 problems, each
having solutions in 5 languages)". It keeps public tests (in the prompt) apart from hidden ones,
reports pass@k with n = 200 and k ∈ {1, 10, 100}, and its translation task removes the description
"to prevent the model from directly solving the problem". Flaw, from reading `execution.py` rather
than a published critique: a JavaScript sample passes only when stdout and stderr are both empty, so
a stray debug print fails a correct program.

**McEval** ([arXiv 2406.07436](https://arxiv.org/abs/2406.07436);
[repo](https://github.com/MCEVAL/McEval); code MIT and data CC-BY-SA-4.0 per the repo page, raw
LICENSE files **UNVERIFIED**) has "40 programming languages ... with 16K test samples": 2,007
generation questions with 10,086 test cases, plus explanation and completion tasks. Ten annotators
wrote it, with a quality target of "90% accuracy", so up to about 10% residual error is tolerated by
design. Models do worse "in functional and procedural programming languages (low-resource
Languages)". Per-model rows beyond GPT-4o's 65.2 average are **UNVERIFIED**.

**BigCodeBench** ([arXiv 2406.15877](https://arxiv.org/abs/2406.15877);
[repo](https://github.com/bigcode-project/bigcodebench), Apache-2.0; dataset Apache-2.0) has 1,140
Python tasks calling "139 libraries and 7 domains", with "5.6 test cases with an average branch
coverage of 99%". It has a Complete split (docstring), an Instruct split (short instruction) and a
Hard subset of 148 tasks. GPT-4o reached about 60% on Complete and under 50% on Instruct, against a
"human performance of 97%". That human figure is 32 of 33 sampled tasks, a solvability check rather
than a baseline, and the authors had to resolve "LLM-generated flaky tests".

**CRUXEval** ([arXiv 2401.03065](https://arxiv.org/abs/2401.03065);
[repo](https://github.com/facebookresearch/cruxeval), MIT) predicts the output (CRUXEval-O) or input
(CRUXEval-I) of "800 Python functions (3-13 lines)". Code Llama 34B generated 102,000 functions;
they were filtered by length (75–300 characters), no floats or true division, a 2-second limit and
no uncaught exceptions, and 800 were sampled from what passed. GPT-4 with chain of thought scored 75%
and 81% on input and output prediction, Code Llama 34B 50% and 46%; "many recent high-scoring models
on HumanEval do not show the same improvements". The authors checked that the generating model
gained no advantage on its own data.

**EvalPlus** ([arXiv 2305.01210](https://arxiv.org/abs/2305.01210);
[repo](https://github.com/evalplus/evalplus), Apache-2.0; HumanEval+ and MBPP+ datasets Apache-2.0)
keeps the tasks and strengthens the tests. Method: about 30 ChatGPT seed inputs per task, then
"type-aware mutation to generate 1000 additional inputs"; contracts discard invalid inputs (that 83
of 164 tasks got them is **UNVERIFIED**); a set-cover reduction gives a 47× smaller "mini" suite.
Numbers: tests "by 80×" on HumanEval; pass@k fell "by up-to 19.3-28.9%" (GPT-4 13.1%, ChatGPT
12.6%); "18 defects (11% of problems) even in the original ground-truth in HumanEval" — 5 miss corner
cases, 10 implement the wrong behaviour, 3 are too slow. MBPP+ removed broken tasks (399 to 378),
and the changelog lists later oracle fixes. The paper gives HumanEval+ both 764.1 and 774.8 average
tests.

**LiveCodeBench** ([arXiv 2403.07974](https://arxiv.org/abs/2403.07974);
[repo](https://github.com/LiveCodeBench/LiveCodeBench), MIT) collects LeetCode, AtCoder and
Codeforces problems over time and scores any window of release dates, in four scenarios: generation,
self-repair, code execution and test-output prediction. Tests average "about 17", some generated by
GPT-4-Turbo. Contamination shows as "a stark drop in the performance of DeepSeek, GPT-4-O, and
Codestral on LeetCode problems released after" their cutoffs; overfitting as DeepSeek-Coder-Instruct
1.3B scoring 60 on HumanEval+ and 26 on LCB-Easy. Releases (README), each from May 2023: v1 400
problems, v4 713, v5 880, v6 (to April 2025) 1,055. The code is MIT but the
[dataset card](https://huggingface.co/datasets/livecodebench/code_generation_lite) says only `cc`,
with no variant. [ERRATA.md](https://raw.githubusercontent.com/LiveCodeBench/LiveCodeBench/main/ERRATA.md)
lists problems accepting multiple answers, 2 interactive problems that "cannot be solved correctly",
and problems with erroneous tests.

**Quality of HumanEval and MBPP.** EvalPlus notes docstrings "too vague to fully clarify the expected
program behaviors". MBPP has "3 automated test cases" per problem, and its authors ship a
hand-verified `sanitized-mbpp.json` (size **UNVERIFIED**). Matton et al.
([arXiv 2407.07565](https://arxiv.org/abs/2407.07565)) separate direct leakage, indirect leakage
"through the use of synthetic data" and "overfitting to evaluation sets during model selection",
and release LBPP, 161 new prompts.

### A.2 Contamination of translated benchmarks

Riddell et al. ([arXiv 2403.04811](https://arxiv.org/html/2403.04811)): for HumanEval "18.9% have
been seen by models trained on the Stack", for MBPP "20.8% of the solutions". On the 10% of MBPP
questions most similar to its training data, StarCoderBase-15.5B scored 72%, against 22% on the
least similar 10%. Cao et al. ([arXiv 2403.16898](https://arxiv.org/abs/2403.16898)): "Refactoring
did not always result in decreased performance; it could lead to improvements"; models "do not
necessarily perform worse on data after the models' cut-off date"; perplexity "cannot distinguish
contaminated/cleansed data". This study's inference, not a cited result: translating a task into
LotML changes its surface, not its algorithm, which a Qwen model may recall (`rl-environment.md`,
arXiv 2507.10532). In a paired LotML-versus-Python comparison the recall helps both arms, so the
difference still isolates the language; an absolute LotML pass@1 overstates problem-solving ability.

### A.3 Agentic and repository benchmarks

**SWE-bench** ([arXiv 2310.06770](https://arxiv.org/abs/2310.06770);
[repo](https://github.com/SWE-bench/SWE-bench), MIT) has 2,294 issues from 12 Python repositories.
The model's patch is applied and the repository's tests run; a task is resolved only "if all tests
across FAIL_TO_PASS and PASS_TO_PASS pass". Gold patches average 1.7 files and 32.8 lines. It needs
Docker on x86_64 with "120GB of free storage, 16GB of RAM, and 8 CPU cores".

- **SWE-bench Verified** keeps 500 tasks engineers "confirmed are solvable" (README). OpenAI's
  announcement gives 1,699 annotated samples, 38.3% flagged as underspecified, 61.1% for tests that
  may reject valid solutions and 68.3% filtered — **UNVERIFIED**: openai.com refused every fetch, so
  they were seen only in a search index, like a February 2026 post saying OpenAI no longer evaluates
  on Verified.
- **SWE-bench Multilingual** ([page](https://www.swebench.com/multilingual.html)): 300 tasks in 9
  languages; the page says 42 repositories, the
  [card](https://huggingface.co/datasets/SWE-bench/SWE-bench_Multilingual) 41.
- **Multi-SWE-bench** ([arXiv 2504.02605](https://arxiv.org/abs/2504.02605), Apache-2.0): 1,632
  instances in 7 languages, kept from 2,456 candidates by 68 annotators, plus 4,723 for RL.
- **Audits.** SWE-bench+ ([arXiv 2410.06992](https://arxiv.org/abs/2410.06992)): "32.67% of the
  successful patches involve cheating as the solutions were directly provided in the issue report",
  "31.08% of the passed patches are suspicious ... due to weak test cases", and SWE-Agent with GPT-4
  "dropped from 12.47% to 3.97%". PatchDiff ([arXiv 2503.15223](https://arxiv.org/abs/2503.15223)):
  "29.6% plausible patches induce different behavior than the ground truth patches", inflating
  resolution rates by "6.2 absolute percent points". The SWE-Bench Illusion
  ([arXiv 2506.12286](https://arxiv.org/abs/2506.12286)): models locate the buggy file "up to 76%"
  of the time "using only issue descriptions", against "up to 53%" outside SWE-bench's repositories.

**Aider polyglot** ([post](https://aider.chat/2024/12/21/polyglot.html);
[repo](https://github.com/Aider-AI/polyglot-benchmark), no LICENSE file, exercises "copyright ©
Exercism") keeps 225 of 697 Exercism exercises, those "solved by 3 or fewer models" of 7: C++ 26,
Go 39, Java 47, JavaScript 49, Python 34, Rust 30. The model gets two tries, the second after the
test errors, and "never gets to see the source code of the unit tests"
([benchmarks page](https://aider.chat/docs/benchmarks.html)). The leaderboard reports "Percent
correct" next to "Percent using correct edit format". The README says to run it "inside a docker
container".

**Terminal-Bench 2.0** ([arXiv 2601.11868](https://arxiv.org/abs/2601.11868);
[tasks](https://github.com/laude-institute/terminal-bench-2) and
[Harbor](https://github.com/laude-institute/harbor), Apache-2.0) has 89 tasks; "each task features a
unique environment, human-written solution, and comprehensive tests", and frontier agents "score
less than 65%". A task is a `task.toml` (verifier and agent timeouts, CPUs, memory, an
`allow_internet` flag), `environment/Dockerfile`, the oracle `solution/solve.sh`, and `tests/test.sh`,
which writes 1 or 0 to `/logs/verifier/reward.txt`. The paper keeps 89 of 229 submitted tasks, runs
each model–agent pair at least 5 times, and uses an adversarial agent that tries to exploit the tests
during review (process figures from one extraction of the HTML).

### A.4 Frameworks

- **bigcode-evaluation-harness** ([repo](https://github.com/bigcode-project/bigcode-evaluation-harness),
  Apache-2.0) ships HumanEval(+), MBPP(+), APPS, DS-1000, HumanEvalPack and MultiPL-E. Code runs only
  with `--allow_code_execution`, "off by default"; the recommended split generates with
  `--generation_only` on the host and evaluates in a container that mounts the generations read-only.
- **lm-evaluation-harness** ([repo](https://github.com/EleutherAI/lm-evaluation-harness), MIT): code
  tasks run only with `--confirm_run_unsafe_code`, with no built-in sandbox; standard errors come
  from 100,000 bootstrap iterations by default.
- **Inspect AI** ([docs](https://inspect.aisi.org.uk), [repo](https://github.com/UKGovernmentBEIS/inspect_ai),
  MIT; [inspect_evals](https://github.com/UKGovernmentBEIS/inspect_evals), MIT) puts sandboxes behind
  one `exec` interface: `docker`, and `local`, which "always executes as the current effective user";
  k8s, Modal and others are extensions. `stderr(cluster=...)` cites Miller's Appendix A in its
  source, `ci_wilson` is for 0/1 scores, and epochs reduce repeated samples per sample (by `mean`)
  before metrics. Bundled evals include HumanEval (5 epochs, Docker), MBPP, BigCodeBench,
  LiveCodeBench Pro and SWE-bench; Harbor tasks run through an adapter.

### A.5 Sandboxed execution

| tool | licence | isolation | host |
| --- | --- | --- | --- |
| [E2B](https://github.com/e2b-dev/E2B) / [infra](https://github.com/e2b-dev/infra) | Apache-2.0 | "one Firecracker microVM per sandbox, in its own cgroup and network namespace" | self-host needs Linux with KVM; otherwise a cloud service |
| [firejail](https://github.com/netblue30/firejail) | GPL-2.0 | SUID; namespaces, seccomp-bpf, capabilities | Linux |
| [nsjail](https://github.com/google/nsjail) | Apache-2.0 | namespaces, rlimits, cgroups, seccomp-bpf (Kafel) | Linux |
| [gVisor](https://github.com/google/gvisor) | Apache-2.0 | user-space kernel, OCI runtime `runsc` | Linux 5.6+ |
| [bubblewrap](https://github.com/containers/bubblewrap) | LGPL-2.0-or-later (source headers; the API reports NOASSERTION) | unprivileged user namespaces | Linux |
| [SandboxFusion](https://github.com/bytedance/SandboxFusion) | Apache-2.0 | default "No Isolation"; "Light Isolation" (cgroups, network and PID namespaces, overlayfs, chroot) needs privileged containers | Linux; 23 languages per [FullStack Bench](https://arxiv.org/abs/2412.00535) |
| [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects) | Windows API | memory, CPU rate, kill-on-close for a process tree; no file or network control | Windows (used by `harness/lotml_harness/limits.py`) |
| [AppContainer](https://learn.microsoft.com/en-us/windows/win32/secauthz/appcontainer-isolation) | Windows API | file, network, device and process isolation, granted per capability | Windows |
| [Windows Sandbox](https://learn.microsoft.com/en-us/windows/security/application-security/application-isolation/windows-sandbox/) | Windows feature | disposable hypervisor VM; networking can be disabled, folders mapped read-only | Windows Pro, Enterprise and Education; one instance at a time; `wsb exec` returns no process output |

### A.6 Statistics of pass@k and comparisons

- **Estimator** ([Chen et al., arXiv 2107.03374](https://arxiv.org/abs/2107.03374)): per task
  pass@k = 1 − C(n−c, k)/C(n, k), averaged over tasks; Codex used "n=200 and k≤100". The naive
  1 − (1 − p̂)^k "is biased", and the gap "doesn't fully close even when n>5k". The stable form is
  `1 - prod(1 - k/i for i in n-c+1..n)`. The definition goes back to Kulal et al.'s "success rate at
  B" ([arXiv 1906.04908](https://arxiv.org/abs/1906.04908)).
- **Miller, "Adding Error Bars to Evals"** ([arXiv 2411.00640](https://arxiv.org/abs/2411.00640)):
  clustered standard errors when questions are related; resampling answers per question, with
  Var(μ̂) = (Var(x) + E[σᵢ²]/K)/n; analysis on question-level paired differences, "a 'free' reduction
  in estimator variance"; and power analysis. Worked example: "n=198, α=0.05, and β=0.20 ...
  increasing K_A=K_B from 1 to 10 reduces the Minimum Detectable Effect from 13.2% to 7.5%", under
  assumed variances. Detecting 3 points needs about 969 questions. Bootstrapping is "unnecessary
  unless a complicated sampling scheme or estimator is being used".
- **Bowyer et al.** ([arXiv 2503.01747](https://arxiv.org/abs/2503.01747)): at N = 100, 95% CLT
  intervals cover "only 92.5%"; they recommend "WS or Bayesian intervals" (Wilson score). For paired
  comparisons at small N "all non-Bayesian methods severely underperform", and a paired Bayesian
  model gives narrower calibrated intervals ([bayes_evals](https://github.com/sambowyer/bayes_evals),
  licence **UNVERIFIED**). McNemar's test is not discussed.
- **Variance across runs.** Madaan et al. ([arXiv 2406.10229](https://arxiv.org/abs/2406.10229)):
  HumanEval's standard deviation over ten Llama-2-7B pretraining seeds is 1.11 points, below its 95%
  CI width of 3.98. Hochlehnert et al. ([arXiv 2504.07086](https://arxiv.org/abs/2504.07086)):
  "Pass@1 values show surprisingly high standard deviation—ranging from 5 to 15 percentage points
  across seeds" on 30-problem AIME, and one model scored 7.3% on one cluster and 11.3% on RunPod; they
  recommend fixed hardware and software and many seeds.
- **Consistency.** pass^k = E[C(c, k)/C(n, k)], all k succeed
  ([τ-bench, arXiv 2406.12045](https://arxiv.org/abs/2406.12045)); G-Pass@k_τ interpolates between
  pass@k (τ → 0) and pass^k (τ = 1) ([arXiv 2412.13147](https://arxiv.org/abs/2412.13147)).
- **Signal and noise** ([arXiv 2508.13144](https://arxiv.org/abs/2508.13144)): noise is measured as
  the spread over the final training checkpoints; signal-to-noise correlates with decision accuracy
  at R = 0.791; averaging checkpoints improved decisions.
- **Paired tests.** For algorithms that run once, Dietterich
  ([Neural Computation 1998](https://mlanthology.org/neco/1998/dietterich1998neco-approximate/))
  finds McNemar's test "the only test with acceptable type I error"; the difference-of-proportions
  test "should never be used".

### A.7 Comparison table

| harness | unit | size | languages | oracle | contamination control | licence (code / data) |
| --- | --- | --- | --- | --- | --- | --- |
| MultiPL-E | function from docstring | HumanEval + MBPP, translated | 18 (paper); 22–24 (HF card) | compiled Python tests | none | BSD-3 + ML restriction / MIT on HF |
| HumanEval-X | function; translation | 820 | 5 | hand-rewritten tests | none | Apache-2.0 / Apache-2.0 |
| McEval | generation, explanation, completion | 16,031 | 40 | human-written tests | new human-written tasks | MIT / CC-BY-SA-4.0 (UNVERIFIED) |
| BigCodeBench | library-heavy function | 1,140 (Hard 148) | Python | 5.6 tests, 99% branch coverage | none | Apache-2.0 / Apache-2.0 |
| CRUXEval | predict output or input | 800 | Python | execution | model-generated functions | MIT / MIT |
| EvalPlus | function | 164 + 378 | Python | ×80 / ×35 tests, fixed ground truth | none | Apache-2.0 / Apache-2.0 |
| LiveCodeBench | contest problem, 4 scenarios | 1,055 (v6) | Python | ~17 tests, some generated | release-date windows | MIT / `cc` (variant unstated) |
| SWE-bench (+ Verified) | issue in a repository | 2,294 (500) | Python | FAIL_TO_PASS + PASS_TO_PASS | none; audited | MIT |
| SWE-bench Multilingual | issue | 300 | 9 | same | none | MIT |
| Multi-SWE-bench | issue | 1,632 | 7 | same | none | Apache-2.0 / CC0 (card text) |
| Aider polyglot | exercise, two tries | 225 | 6 | hidden Exercism tests | hard-only selection | none (Exercism content) |
| Terminal-Bench 2.0 | task in a container | 89 | any | pytest writes a reward file | exploit agent in review | Apache-2.0 |

## Topic B — teaching a model a new or low-resource language

### B.1 In context: grammars, books and documentation

- **Grammar prompting** ([arXiv 2305.19234](https://arxiv.org/abs/2305.19234);
  [repo](https://github.com/berlino/grammar-prompting), no licence): each example carries the minimal
  BNF fragment that derives it; the model predicts a grammar, then the program. Codex execution
  accuracy, standard to grammar prompting (with constrained decoding): GeoQuery 81.5 to 87.5 (88.9),
  SMCalFlow 46.4 to 50.8 (52.4), Overnight-Blocks 54.7 to 57.4 (60.9). Not uniform: GPT-3.5 on
  SMCalFlow fell from 9 to 5, PaLM 2-L on GeoQuery from 90 to 87. It "did not yield any improvements
  for DSLs that were likely to have been frequently encountered during pretraining (e.g., regular
  expressions, SQL)" — LotML is the opposite case.
- **One grammar book.** MTOB ([arXiv 2309.16575](https://arxiv.org/abs/2309.16575)) translated
  Kalamang from a 573-page grammar in context; the best model reached 44.7 and 45.8 chrF against a
  human's 51.6 and 57.0, and "sentences (S) are the most beneficial, followed by entries from the word
  list (W), followed by excerpts from the grammar book". Aycock et al.
  ([arXiv 2409.19151](https://arxiv.org/abs/2409.19151), ICLR 2025;
  [repo](https://github.com/Sethjsa/XLR-MTOB), MIT) split the book and found "almost all improvements
  stem from the book's parallel examples rather than its grammatical explanations". Gemini-1.5-Flash,
  English to Kalamang chrF++: 11.0 zero-shot, 30.8 with the parallel sentences only, 22.6 with the
  explanations only. Explanations did help grammaticality judgment and glossing. Natural language,
  not code, but the controlled version of the wiki's CangjieBench and PyLang finding.
- **DocPrompting** ([arXiv 2207.05987](https://arxiv.org/abs/2207.05987);
  [repo](https://github.com/shuyanzhou/docprompting), Apache-2.0) retrieves the documentation of
  functions unseen in training, then generates. On CoNaLa, CodeT5 gained "2.85% in pass@1 (52%
  relative gain)"; on tldr (Bash), up to 6.9 points of exact match. Codex rows are **UNVERIFIED**
  (garbled table).

### B.2 Designing the language for the model: MoonBit

The paper ([LLM4Code 2024, DOI 10.1145/3643795.3648376](https://dl.acm.org/doi/10.1145/3643795.3648376))
describes a flat design and a "real-time, semantics-based sampler"; its full text was not
retrievable, so its numbers are **UNVERIFIED**. The [official blog](https://moonbitlang.com/blog/moonbit-ai)
adds mandatory type signatures on top-level definitions ("more KV-cache friendly"), local (syntax)
and global (type) sampling, and "significant improvement in compilation rates, with a modest
performance penalty of approximately 3%", with no absolute rates. Later posts (MoonBit Pilot, July
2025) are vendor demonstrations. Nothing citable measures either lever's effect.

### B.3 Transfer between languages, and where a small model starts

- **IRCoder** ([arXiv 2403.03894](https://arxiv.org/abs/2403.03894), ACL 2024) continued training on
  paired source and LLVM IR (SLTrans, about 3.9M files, 26.2B tokens). MultiPL-E average,
  DeepSeek-Coder-1.3B: 18.34 to 20.51; ablation: source only 18.22, unpaired IR 18.77, paired 20.51.
  "Grounding ... in the same IR accounts for the majority of performance gains, and not the mere
  exposure to IR." Gains are uneven and some languages fell. The dataset card says cc-by-sa-4.0 while
  the paper reportedly says CC BY-NC-SA 4.0; the code licence is **UNVERIFIED**.
- **Cross-lingual transfer** ([arXiv 2310.16937](https://arxiv.org/abs/2310.16937), TMLR 2025),
  CodeT5-base across 41 languages: "Kotlin is the best source language over all target languages and
  tasks. JavaScript is the best source language for low-resource target languages." "Keywords and
  names are top features"; for generation, "expression of types and overlap in literals dominate".
- **Survey** ([arXiv 2410.03981](https://arxiv.org/abs/2410.03981), TOSEM) of 111 papers: "a
  standard approach and benchmark dataset for evaluating code generation in LRPLs and DSLs are
  lacking".
- **Qwen2.5-Coder baselines** ([arXiv 2409.12186](https://arxiv.org/abs/2409.12186)), MultiPL-E:
  base 0.5B Python 28.0, Bash 7.0, average 24.7; base 7B average 57.5, Bash 38.6; 0.5B-Instruct
  average 49.6, Bash 27.8. The 0.5B is weak even on its least frequent seen language; no McEval or
  Lua/R/Racket numbers are given for 0.5B–7B.

### B.4 Synthetic data: translate, test, keep

- **MultiPL-T** ([arXiv 2308.09895](https://arxiv.org/abs/2308.09895), local cache;
  [repo](https://github.com/nuprl/MultiPL-T), BSD-3-Clause;
  [dataset](https://huggingface.co/datasets/nuprl/MultiPL-T), bigcode-openrail-m, gated). Python
  filtering: 22,311,478 functions, 5,359,051 with docstrings, 459,280 that type-check and return a
  value, 432,361 after removing TODOs and benchmark solutions. Tests: StarCoder-15B wrote "five
  independent test suites for each function", failing tests were discarded, 157,767 functions got
  tests and 133,168 reached 90% line coverage (the text also says 133,668). Translation: "50
  translations with high temperature (0.8)" per function (100 for OCaml), then ROUGE-L deduplication
  at 0.6. Final sets: R 37,592, Racket 40,510, OCaml 43,401, Julia 45,000, Lua 48,194. Lua: 1B 12.1
  to 17.3, 15B 26.6 to 31.0. R: 1B 5.4 to 8.9, 15B 10.2 to 17.3. Python source in the translator's
  prompt (pass@50): Racket 34.7 to 56.8, Lua 51.4 to 68.5, OCaml 26.1 to 23.9.
- **Q, full-stack fine-tuning** ([arXiv 2508.06813](https://arxiv.org/abs/2508.06813); code under
  [morganstanley/MSML](https://github.com/morganstanley/MSML), licence **UNVERIFIED**). LeetCode
  problems translated from Python by Qwen-2.5-32B and verified by execution, 678 problems (542 train,
  136 test), plus 1.6M tokens of pretraining; multi-task SFT (description to Q, Q to Python, Python
  to Q), then GRPO. Base Qwen-2.5 on Description-to-Q: 1.5B, 3B and 7B 0.0%, 14B 0.7%, 32B 6.6%.
  Best: "pass@1 accuracy of 59 percent ... surpassing ... Claude Opus-4 by 29.5 percent". At 1.5B:
  "RL adaptation ... did not yield meaningful improvements, in fact, under our GRPO setup, pass rates
  typically declined after RL". Generating tests and solutions jointly let "the model ... exploit the
  evaluation by generating trivial or overfitted test cases".
- **AlphaVerus** ([arXiv 2412.06176](https://arxiv.org/abs/2412.06176);
  [repo](https://github.com/cmu-l3/alphaverus), licence **UNVERIFIED**) translates Dafny to Verus with
  Llama-3.1-70B and no fine-tuning: exploration, tree search over verifier errors, then a critique
  filter. Of 562 programs it kept 247 translations and 579 exploit pairs; without the critique it
  succeeded "primarily because of learning to use assume (false)". The HumanEval-Verus score is
  **UNVERIFIED** (32.9% and 38% in two sources).
- **OSS-Instruct** ([arXiv 2312.02120](https://arxiv.org/abs/2312.02120);
  [repo](https://github.com/ise-uiuc/magicoder), MIT) seeds generation with 80K open-source snippets
  (40K Python, 5K each of eight others). Its per-language MultiPL-E numbers are **UNVERIFIED**: three
  extractions disagreed.

### B.5 RL and related training from compiler and test feedback

- **StepCoder** ([arXiv 2402.01391](https://arxiv.org/abs/2402.01391); APPS+ data
  [repo](https://github.com/Ablustrund/APPS_Plus), MIT). PPO with +1 pass, −0.3 test fail, −0.6
  runtime error, −1 compile error. Its CCCS curriculum first has RL complete only the tail of a
  canonical solution, then starts earlier and earlier; FGO masks unexecuted code. DeepSeek-Coder-
  Instruct 6.7B on APPS+ (7,456 problems; the repo says 7,413): 29.2 base, 31.7 vanilla PPO, 36.1
  StepCoder; without CCCS 34.6, without FGO 35.5.
- **RLTF** ([arXiv 2307.04349](https://arxiv.org/abs/2307.04349); [repo](https://github.com/Zyq-scut/RLTF),
  BSD-3-Clause): online RL with penalties on the error line and an adaptive −0.3 + 1.3 × pass ratio.
  CodeGen 2.7B on APPS pass@1: 1.64 to 2.04. Its authors find the manual error categories hard "to
  transfer ... to other programming languages".
- **Compile-rate rewards.** PPOCoder ([arXiv 2301.13816](https://arxiv.org/abs/2301.13816);
  [repo](https://github.com/reddy-lab-code-research/PPOCoder), MIT): CodeT5-220M completion compile
  rate 52.14 to 97.68, exact match 42.61 to 42.63; translation to C, the scarcest target, rose least
  (42.71 to 48.82). COMPCODER ([arXiv 2203.05132](https://arxiv.org/abs/2203.05132)): compilation
  "from 44.18 to 89.18 in code completion", with a reranker by predicted compilability adding most of
  the rest. RLCF ([arXiv 2305.18341](https://arxiv.org/abs/2305.18341)): CodeGen-350M on Java MBJP,
  pass@1 4.54 to 5.08, compile rate 60.68 to 71.82.
- **DeepSeek-Prover-V1.5** ([arXiv 2408.08152](https://arxiv.org/abs/2408.08152);
  [repo](https://github.com/deepseek-ai/DeepSeek-Prover-V1.5), code MIT): GRPO in Lean 4, "a reward of
  1 if verified as correct, and 0 otherwise", on "approximately 4.5k" theorems the SFT model solves at
  "a moderate success rate". 7B, miniF2F pass@128: base 29.7, SFT 50.4, RL 51.6; ProofNet 9.7, 15.9,
  18.2.
- **CodeV-R1** ([arXiv 2505.24183](https://arxiv.org/abs/2505.24183);
  [repo](https://github.com/IPRC-DIP/CodeV-R1), no licence): SFT on about 87K R1 traces, then DAPO on
  about 3.1K problems with a binary testbench-equivalence reward. Qwen2.5-Coder-7B-Instruct,
  VerilogEval2-SR: 31.3 to 65.2 (SFT) to 68.8 (RL); RTLLM v2: 36.1 to 57.2 to 68.0.
- **Prolog GRPO** ([arXiv 2506.11027](https://arxiv.org/abs/2506.11027);
  [repo](https://github.com/biancaraimondi/LLM_Format), Apache-2.0): +1 correct, −1 logic error, −0.5
  syntax error, −0.1 timeout. "Qwen2.5-Coder-0.5B, is initially not able to learn in zero-shot
  settings" (0.00); "a single demonstration (one-shot) serves as a syntax anchor", reaching 0.13 after
  1,000 steps. 7B one-shot: 0.814 to 0.893 after reward fixes.
- **Small models on Python** ([arXiv 2605.30478](https://arxiv.org/abs/2605.30478)): LoRA GRPO or GSPO
  on 374 MBPP tasks. Qwen3-0.6B 0.273 to 0.417 (GSPO, combined test and lint reward), Llama3.2-1B
  0.349 to 0.389; the abstract says "up to 13 percentage points", the table +14.4. Lint-only
  penalties "may bias the policy toward shorter completions".
- **Multi-turn.** RLEF ([arXiv 2410.02089](https://arxiv.org/abs/2410.02089), local cache): Llama 3.1
  8B on CodeContests test 1@3, 10.5 to 16.0, with 12,000 updates on "288 ... GPUs for 8B" for about
  20 hours; before training, multi-turn was worse than single-turn (10.5 against 11.8). MURPHY
  ([arXiv 2511.07833](https://arxiv.org/abs/2511.07833)): Qwen3-1.7B HumanEval after 3 iterations,
  80.07 base, 82.11 multi-turn GRPO, 86.58 MURPHY. μCode ([arXiv 2502.20380](https://arxiv.org/abs/2502.20380))
  is imitation, not RL: Llama-3.2-1B HumanEval 25.6 to 35.4.
- **Agnostics, beyond the wiki** ([arXiv 2508.04865](https://arxiv.org/abs/2508.04865), local cache;
  the [framework](https://github.com/nuprl/agnostics-framework) has **no licence**): Ag-LiveCodeBench-X
  is the 499 stdin/stdout problems of LiveCodeBench 5.0's 880. GRPO, no KL, "4 prompts in each batch,
  with group size 32 per prompt", on 4–8 H100s. OCaml and Fortran got tips o3 distilled from failing
  samples, used only in training; "base accuracy as low as 0.09% was enough for the model to start
  learning". Qwen3-4B on Ag-LiveCodeBench-X: R 10 to 15, Fortran 0 to 15. Qwen3-8B: Lua 11 to 25,
  Julia 9 to 25, OCaml 0 to 7.
- **Rust with cargo rewards** (Oxen.ai [blog](https://ghost.oxen.ai/training-a-rust-1-5b-coder-lm-with-reinforcement-learning-grpo/),
  not peer-reviewed; [repo](https://github.com/Oxen-AI/GRPO-With-Cargo-Feedback), MIT):
  Qwen2.5-Coder-1.5B-Instruct on one H100 for about 24 hours, build rate 61% to 80%, tests 22% to 37%.
  The model wrote its own tests, and the authors warn of making "the functions and unit tests trivial".

### B.6 Results table

| approach | source | model | language | before → after |
| --- | --- | --- | --- | --- |
| grammar in prompt | 2305.19234 | Codex | GeoQuery DSL | 81.5 → 87.5 |
| examples vs explanations | 2409.19151 | Gemini-1.5-Flash | Kalamang | 11.0 → 30.8 (examples) / 22.6 (rules) |
| doc retrieval | 2207.05987 | CodeT5 | Python (CoNaLa) | +2.85 pass@1 |
| paired IR pretraining | 2403.03894 | DeepSeek-Coder 1.3B | MultiPL-E average | 18.34 → 20.51 |
| translate and validate | 2308.09895 | StarCoderBase 1B | Lua / R | 12.1 → 17.3 / 5.4 → 8.9 |
| pretrain + SFT + RL | 2508.06813 | Qwen-2.5 1.5B–32B | Q | 0.0–6.6% base; best 59%; RL at 1.5B declined |
| RL, binary I/O reward | 2508.04865 | Qwen3-4B | Fortran (LCB-X) | 0 → 15 |
| RL, graded reward | 2506.11027 | Qwen2.5-Coder 0.5B | Prolog | 0.00 → 0.00 zero-shot; → 0.13 one-shot |
| RL, test + lint | 2605.30478 | Qwen3-0.6B | Python (MBPP) | 27.3 → 41.7 |
| PPO + curriculum | 2402.01391 | DeepSeek-Coder 6.7B | Python (APPS+) | 29.2 → 36.1 |
| compile reward | 2301.13816 | CodeT5 220M | Python (compile rate) | 52.14 → 97.68 |
| compile + discriminator | 2305.18341 | CodeGen 350M | Java (MBJP) | 4.54 → 5.08 |
| SFT then RL, checker | 2408.08152 | 7B | Lean 4 (miniF2F pass@128) | 29.7 → 50.4 → 51.6 |
| SFT then RL, testbench | 2505.24183 | Qwen2.5-Coder 7B | Verilog | 31.3 → 65.2 → 68.8 |
| multi-turn RL | 2410.02089 | Llama 3.1 8B | Python (CodeContests 1@3) | 10.5 → 16.0 |
| multi-turn RL | 2511.07833 | Qwen3-1.7B | Python (HumanEval, iter-3) | 80.07 → 86.58 |
| RL, cargo reward (blog) | Oxen.ai | Qwen2.5-Coder 1.5B | Rust (tests) | 22% → 37% |

## Reference repositories

| repository | licence | why | what to borrow |
| --- | --- | --- | --- |
| [nuprl/MultiPL-E](https://github.com/nuprl/MultiPL-E) | BSD-3 + ML restriction | the translation recipe the task set follows | `--network none` container; evaluation only |
| [evalplus/evalplus](https://github.com/evalplus/evalplus) | Apache-2.0 | stronger HumanEval/MBPP tests, corrected ground truth | type-aware input mutation; the fixed oracles; licence allows training |
| [LiveCodeBench/LiveCodeBench](https://github.com/LiveCodeBench/LiveCodeBench) | MIT (data `cc`) | date windows | per-window scores; ERRATA exclusions |
| [bigcode-project/bigcodebench](https://github.com/bigcode-project/bigcodebench) | Apache-2.0 | test-strength reporting | branch coverage per task; Complete versus Instruct pairing |
| [facebookresearch/cruxeval](https://github.com/facebookresearch/cruxeval) | MIT | corpus-free reasoning probe | generation filters for an output-prediction set |
| [SWE-bench/SWE-bench](https://github.com/SWE-bench/SWE-bench) | MIT | repository tasks | FAIL_TO_PASS + PASS_TO_PASS contract |
| [laude-institute/harbor](https://github.com/laude-institute/harbor), [terminal-bench-2](https://github.com/laude-institute/terminal-bench-2) | Apache-2.0 | task packaging | `task.toml`, oracle run, reward file, per-task limits |
| [bigcode-project/bigcode-evaluation-harness](https://github.com/bigcode-project/bigcode-evaluation-harness) | Apache-2.0 | generation and execution split | generate on the host, execute in a read-only mount |
| [EleutherAI/lm-evaluation-harness](https://github.com/EleutherAI/lm-evaluation-harness) | MIT | opt-in gates | an explicit unsafe-execution flag |
| [UKGovernmentBEIS/inspect_ai](https://github.com/UKGovernmentBEIS/inspect_ai) | MIT | sandbox and metrics design | `exec` interface; clustered `stderr`; epochs |
| [google/nsjail](https://github.com/google/nsjail) | Apache-2.0 | Linux grader | rlimits + seccomp per candidate |
| [bytedance/SandboxFusion](https://github.com/bytedance/SandboxFusion) | Apache-2.0 | execution service | API shape for an RL grader |
| [nuprl/MultiPL-T](https://github.com/nuprl/MultiPL-T) | BSD-3-Clause (data openrail-m, gated) | corpus pipeline | test generation, coverage filter, ROUGE-L dedup |
| [nuprl/agnostics-framework](https://github.com/nuprl/agnostics-framework) | none | RL environment for any language | reimplement; do not copy |
| [Sethjsa/XLR-MTOB](https://github.com/Sethjsa/XLR-MTOB) | MIT | examples-vs-rules ablation | its split, applied to the LotML reference |
| [shuyanzhou/docprompting](https://github.com/shuyanzhou/docprompting) | Apache-2.0 | doc retrieval | per-function docs for the standard library |
| [Ablustrund/APPS_Plus](https://github.com/Ablustrund/APPS_Plus) | MIT | curriculum data | CCCS idea (no training code there) |
| [Zyq-scut/RLTF](https://github.com/Zyq-scut/RLTF) | BSD-3-Clause | line-level penalties | ablation design only |
| [deepseek-ai/DeepSeek-Prover-V1.5](https://github.com/deepseek-ai/DeepSeek-Prover-V1.5) | MIT (code) | checker-reward recipe | prompt filtering by SFT pass rate |
| [biancaraimondi/LLM_Format](https://github.com/biancaraimondi/LLM_Format) | Apache-2.0 | GRPO at 0.5B on a rare language | one-shot anchor setup |
| [Oxen-AI/GRPO-With-Cargo-Feedback](https://github.com/Oxen-AI/GRPO-With-Cargo-Feedback) | MIT | toolchain rewards on one GPU | reward plumbing; not its self-written tests |
| [berlino/grammar-prompting](https://github.com/berlino/grammar-prompting), [IPRC-DIP/CodeV-R1](https://github.com/IPRC-DIP/CodeV-R1) | none | methods | read only |

## Implications for LotML

### Harness

1. **Cluster by task.** `harness/lotml_harness/agent/report.py` computes `wilson(passed,
   len(graded))` over every graded run, so several runs of one task count as independent trials.
   Report Wilson only for one run per task; with K > 1, report a t-interval over per-task pass
   fractions or cluster by task, as Miller and Inspect do. Chen's estimator there is already exact
   (`math.comb`).
2. **Pick K from the minimum detectable effect.** At about 200 paired tasks one sample per task
   detects about 13 points and ten detect about 7.5 (Miller's example; this harness's own variances
   should be measured). Paired tasks stay the primary lever. Keep exact McNemar for K = 1
   (Dietterich); for K > 1, test per-task differences (paired standard error or a sign-flip
   permutation test), and report the difference with its interval, not a p-value alone.
3. **State future gates as non-inferiority with a margin fixed before the run.** "No model
   significantly worse" at about 200 pairs cannot exclude a 10-point loss. This is a statistical
   reading, not a cited result, and it does not reopen gates already decided
   (adr:0010-variant-b-and-indented-blocks-settled-by-the-phase-0-gate,
   adr:0011-proceed-to-phase-2-past-the-failed-phase-1-gate): adr:0011 forbids rewriting a gate
   after its result.
4. **Strengthen hidden tests the EvalPlus way.** Generate inputs by type-aware mutation and run
   them differentially against the reference; LotML's two backends make Python versus LLVM output a
   second free oracle. Publish branch coverage per task next to the mutant kill rate the wiki
   already plans.
5. **Cross-check HumanEval ground truth.** Flag the 18 tasks whose canonical solutions EvalPlus
   fixed, and use its corrected oracles (Apache-2.0).
6. **Treat LiveCodeBench with its errata and dates.** Drop or re-judge the problems ERRATA.md lists
   (multiple answers, interactive, wrong tests), report v6 scores by release window relative to each
   model's cutoff, and clarify the dataset's `cc` licence before any training use.
7. **Add a CRUXEval-style output-prediction probe** generated from LotML programs and executed by
   the compiler. It measures whether a model reads LotML's semantics — value semantics, `/` giving
   `f64` (adr:0007-integer-division-returns-f64) — separately from writing: the gap arXiv 2510.03415
   found for familiar syntax with new meaning.
8. **Report three numbers per run, Aider style:** first answer, after feedback, and well-formed
   output (parse or schema). Test sources stay hidden from the agent, as the `hidden:` prefix does.
9. **Package agent tasks like Terminal-Bench:** a manifest with limits, an oracle solution run that
   proves each task solvable, a reward file, and an adversarial pass that tries to make the tests
   pass without solving the task (impossible tasks are in the wiki already).
10. **Isolation.** On Windows, keep the Job Objects of `limits.py` and add an AppContainer profile
    with no network and writes confined to the scratch directory; Windows Sandbox fits only serial
    runs, and `wsb exec` returns no output. The planned Linux grader fits nsjail (Apache-2.0) or
    SandboxFusion, and gVisor fits full agent containers. Any container conflicts with
    adr:0014-deepagents-over-openrouter-for-the-agent-harness as written ("which this harness does
    not provide"): it is a new decision and a `docs/stack.md` entry, not a silent addition.
11. **Freeze the evaluation environment for RL checkpoints:** the same engine (llama-server, per
    adr:0017-a-half-billion-coder-model-tuned-locally-and-served-by-llama-server), quantization,
    decoding, hardware and seeds. Report mean ± std over seeds, pass^k next to pass@k, and the spread
    over the last checkpoints (Hochlehnert, AI2).

### Training

1. **The seed corpus cannot be trained on as built.** `harness/results/corpus.md` translates "each
   task's canonical typed Python" from MultiPL-E; `licences.toml` marks `multipl-e-humaneval` and
   `multipl-e-mbpp` as `training = "forbidden"`, and NOTICE.md excludes them "including for the
   Python-to-lotml corpus" — yet `small-coder-training.md` item 3 calls that corpus "the seed".
   Rebuild it from openai/human-eval (MIT, as adr:0015-pose-humaneval-untyped does) and MBPP's
   original release (CC-BY-4.0 per its [card](https://huggingface.co/datasets/google-research-datasets/mbpp),
   attribution required), with EvalPlus inputs (Apache-2.0).
2. **Scale is the gap.** MultiPL-T kept 37,592–48,194 tested items per language, and Q started from
   678 verified problems plus 1.6M tokens. Test validation is the active ingredient (Racket at 15B
   gained nothing without it). Grow from permissively licensed Python functions with generated
   tests, coverage filters and deduplication.
3. **Pairs, not prose.** Examples carry in-context learning (Aycock, MTOB), and paired data carries
   IRCoder's gains. Build SFT multi-task as Q did: description to LotML, Python to LotML and back,
   and LotML paired with the Python its backend emits
   (adr:0025-two-targets-python-for-run-llvm-for-build). A grammar fragment per example (grammar
   prompting) is a cheap ablation for the reference, since LotML is unseen.
4. **Generate solutions and tests separately.** Q, AlphaVerus and the Rust blog were all gamed when
   the same model wrote both; the grader must own the tests, as `rl-environment.md` requires.
5. **RL at 0.5B needs a nonzero start, built on purpose.** Prolog GRPO at 0.5B learned nothing
   zero-shot, Q's GRPO at 1.5B declined, and Agnostics needed only 0.09% at 4B. Order: SFT first;
   StepCoder's curriculum (complete the tail of a reference program first, so groups carry reward);
   prompts filtered to SFT pass rates in (0, 1) (DeepSeek-Prover; Goedel-Prover-V2,
   [arXiv 2508.03613](https://arxiv.org/abs/2508.03613), used (0, 0.75]); a one-shot anchor in the
   prompt while the pass rate is near zero. Expect a few points over SFT (+1.2 Lean, +4 to +11
   Verilog), not new ability. adr:0017 names the 1.5B as the next size; Q is evidence that 1.5B
   alone does not fix RL.
6. **Keep the binary test reward; `lotml check` is a gate or a tie-breaker, never the objective.**
   Compile-only rewards drove compile rate to 98% with no gain in correctness (PPOCoder, COMPCODER,
   RLCF), and lint-only rewards shortened programs at 0.6B — in line with `verifiable-rewards.md`.
7. **Filter by the checker at inference.** COMPCODER's reranker shows the value of choosing among k
   samples by compilability; LotML's exact `lotml check` does it for free: sample k, discard what
   fails, then run tests. For the 0.5B guide this costs latency, not training.
8. **Leave multi-turn RL for later.** It worked at 1–1.7B (MURPHY, μCode) and 8B (RLEF, on 288
   GPUs), and untrained models did not use the feedback. Single-turn binary GRPO comes first.

### Conflicts and tensions with ADRs

- **adr:0015-pose-humaneval-untyped** takes "the canonical solution's own results" as the oracle;
  EvalPlus shows 18 of those solutions are wrong, so up to 11% of humaneval-original tasks grade
  against defective answers. No supersession is needed if the 18 are checked against EvalPlus's
  fixes and the result recorded.
- **adr:0014-deepagents-over-openrouter-for-the-agent-harness** runs the agent's code with the
  user's privileges and no container, while every external agentic harness surveyed assumes Docker
  on Linux. Borrowing Harbor, Inspect's docker sandbox or SWE-bench's harness needs a new ADR.
- **adr:0017-a-half-billion-coder-model-tuned-locally-and-served-by-llama-server** is consistent with
  the evidence that 0.5B models repair and point rather than author; its fallback, the 1.5B, is not
  supported as an RL fix by Q's result.
- **adr:0010 and adr:0011** stand; the statistical caveat of Harness 3 applies only to gates written
  after this study.
- **No ADR, but a wiki conflict:** `small-coder-training.md` against `licences.toml` and NOTICE.md
  on training from the MultiPL-E-derived corpus (Training 1).

## Sources

Papers, each at `https://arxiv.org/abs/<id>`: Codex/pass@k 2107.03374 · SPoC 1906.04908 · MultiPL-E
2208.08227 · HumanEval-X 2303.17568 · McEval 2406.07436 · BigCodeBench 2406.15877 · CRUXEval
2401.03065 · EvalPlus 2305.01210 · LiveCodeBench 2403.07974 · leakage 2407.07565 · Riddell et al.
2403.04811 · Cao et al. 2403.16898 · SWE-bench 2310.06770 · Multi-SWE-bench 2504.02605 · SWE-bench+
2410.06992 · PatchDiff 2503.15223 · SWE-Bench Illusion 2506.12286 · Terminal-Bench 2601.11868 ·
FullStack Bench 2412.00535 · Miller 2411.00640 · Bowyer et al. 2503.01747 · Madaan et al. 2406.10229
· Hochlehnert et al. 2504.07086 · τ-bench 2406.12045 · G-Pass@k 2412.13147 · Signal and Noise
2508.13144 · grammar prompting 2305.19234 · MTOB 2309.16575 · Aycock et al. 2409.19151 · DocPrompting
2207.05987 · IRCoder 2403.03894 · cross-lingual transfer 2310.16937 · low-resource survey 2410.03981 ·
Qwen2.5-Coder 2409.12186 · MultiPL-T 2308.09895 · Q 2508.06813 · AlphaVerus 2412.06176 · Magicoder
2312.02120 · StepCoder 2402.01391 · RLTF 2307.04349 · PPOCoder 2301.13816 · COMPCODER 2203.05132 ·
RLCF 2305.18341 · DeepSeek-Prover-V1.5 2408.08152 · CodeV-R1 2505.24183 · Prolog GRPO 2506.11027 ·
Goedel-Prover-V2 2508.03613 · Skopin and Kotelnikov 2605.30478 · RLEF 2410.02089 · MURPHY 2511.07833
· μCode 2502.20380 · Agnostics 2508.04865.

Other primary pages — MoonBit's ACM DOI and blog, Dietterich 1998, Aider's posts, the SWE-bench
Multilingual page and Verified card, Inspect's documentation, Microsoft Learn, the MBPP and
MultiPL-E dataset cards, LiveCodeBench's ERRATA, the Oxen.ai blog — are linked inline where cited.
Repository licences, as in the tables, were read from LICENSE files or GitHub on 2026-10-07.
