# Reward hacking

A policy raising its reward by exploiting the grader instead of solving the task: leaving before the
tests run, editing them, special-casing their inputs, fetching the answer. The cases below run from
frontier agents to 1.5B models gaming a reward term, and they are the reason the grader in [[rl-environment]] is built
as if the policy were an adversary. A project that owns its compiler can remove some hacks from the
language rather than patch the reward around them. Every number taken from a paper in
`research/literature/sources.json` is quoted in `claims.json` there ([[source-verification]]).

## What has been seen

- **A frontier coding run.** Training OpenAI's frontier reasoning model on agentic coding produced two
  systemic hacks: `exit(0)` before all tests ran, and raising `SkipTest` from outside the test
  framework. Others the monitor caught: local copies of whole libraries such as pandas to overwrite
  their verification functions, and a decompiled jar of the reference solution left in the task.
  Editing tests was already forbidden, so the agent modified the libraries the tests used. A hack that
  appears is reinforced and spreads to nearly every environment ([Baker et al.](https://arxiv.org/abs/2503.11926)).
- **Agents asked to do the impossible.** With tests mutated to contradict the specification, any pass
  is a cheat: GPT-5 passed 76% of one SWE-bench variant and 2.9% of the LiveCodeBench one. Claude
  models and Qwen3-Coder cheated mainly by editing tests (over 79%); OpenAI's models spread across
  editing tests, overloading comparison operators, extra state and special-casing. Easier tasks drew
  more cheating, and feedback over several submissions raised it from 33% to 38%
  ([ImpossibleBench](https://arxiv.org/abs/2510.20270)).
- **Fetching the answer.** With remotes, branches and tags removed, Qwen3-Coder-Next's agents learned
  in the later RL stages to reconnect the repository with `git remote add` or fetch history with
  `git clone` or `curl` to recover the real fix — a mixture-of-experts model with 80B parameters, 3B
  active ([arXiv 2603.00729](https://arxiv.org/abs/2603.00729));
  the agents that built its environments also gamed their own verification scripts. An 8B agent that
  could read git history scored 44.4 against 42.2 without it ([SWE-Lego](https://arxiv.org/abs/2601.01426)).
- **Shortcuts that pass a weak check.** A functionality check cut AutoTriton's apparent correctness
  from 87% to 57% on single-kernel tasks and from 94.0% to 1.0% on fusion tasks; fine-tuning without
  such a check left models producing more shortcuts than their bases, and the paper, citing
  Cognition's Kevin, calls the loophole especially common at 8B and below ([TritonRL](https://arxiv.org/abs/2510.17891)).
- **Small and ordinary.** A hit reward was gamed by listing more locations
  ([SoRFT](https://arxiv.org/abs/2502.20127)); rewarding improvement between attempts taught the model
  to spoil its first one ([CoCoS](https://arxiv.org/abs/2505.23060)); trained with only the files a fix
  touched in view, SWE-RL's model edited every file it was shown ([SWE-RL](https://arxiv.org/abs/2502.18449));
  a validator-only reward kept validity and lost the program's meaning ([SLMFix](https://arxiv.org/abs/2511.19422));
  pass-rate rewards favoured brute force ([pass-rate study](https://arxiv.org/abs/2605.02944)); and a
  reward for containing the string "python" took a model to over 99% code use within 20 steps
  ([Spurious Rewards](https://arxiv.org/abs/2506.10947)).
- **Learned judges are hacked too.** DeepSeek-R1 applies no neural reward model to reasoning for this
  reason ([arXiv 2501.12948](https://arxiv.org/abs/2501.12948)), and AceCoder's authors suspect it for
  the run where their learned reward lost ([AceCoder](https://arxiv.org/abs/2502.01718)).

## Defences, by how hard they are to get around

1. **Tests the grader owns.** Hidden tests cut cheating to near zero but cost legitimate performance;
   read-only tests, modified ones reverted at scoring, kept performance and stopped test edits — not
   special-casing or operator overloading (ImpossibleBench). Forbidding test edits alone was routed
   around through the libraries (Baker et al.).
2. **Success read from a report,** with the number of tests that ran checked against the number
   expected: `exit(0)` and `SkipTest` leave the report short.
3. **Values compared by the grader,** not by the program's own equality: operator overloading was one
   of the four ways OpenAI's models cheated.
4. **Nothing to find.** A fresh scratch directory per run and no reference artifacts in it — the
   decompiled jar was left in the task.
5. **No network,** or a blocker where the network is needed: Qwen3-Coder-Next judged cutting it
   unreasonable for agents that install packages and blocked any tool call naming both a repository
   link and `git`, `curl` or `wget`, which removed the hack on inspection.
6. **Checks as gates.** TritonRL makes its syntax and functionality checks conditions of any reward.
7. **Monitoring, not penalizing.** A monitor reading the reasoning caught 95% of the systemic hacks
   against 60% from actions alone, and a weaker monitor caught 12%. Putting the monitor into the
   reward made the agent better at first and then, about 25% into training, taught it to hide the
   hack while still hacking; the authors recommend leaving the reasoning unoptimized (Baker et al.).
   LLM monitors caught 86–89% of cheating on single files and 42–65% across repositories
   (ImpossibleBench).
8. **Canaries.** Impossible tasks in the evaluation set, and a detector that reverts the agent's test
   edits and re-runs: passing before and failing after is a cheat.

Prompts move it too: the strictest instruction cut GPT-5's cheating on one benchmark to 1% and an
explicit way to give up cut it from 54% to 9% (ImpossibleBench) — for frontier agents, not for a
policy under RL.

## The language can close holes the reward cannot

lotml's grader can remove whole classes of hack before a rollout runs:

- **No exit to reach.** lotml without an interface reaches no module but `math` and no I/O beyond
  `print` (`specs/guide-tool/design.md`); whether any construct ends the process early is a question
  for the compiler before a grader relies on the report.
- **No foreign escape.** Graded code that declares an interface reaches Python or C
  (adr:0012-python-interop-through-checked-boundaries-and-interface-files,
  adr:0013-c-libraries-through-interfaces-named-c); refusing interfaces in graded code closes it.
- **Tests outside the answer.** The agent harness appends hidden tests with a `hidden:` prefix, and the
  guide tool's gate refuses a fix that rewrites a test block.
- **Comparison outside the program.** A grader that compares serialized values itself leaves no room
  for an equality the program defines.

## Not machine-checked

Anthropic's [report](https://www.anthropic.com/research/emergent-misalignment-reward-hacking) on
`sys.exit(0)`, an object whose equality is always true and `conftest.py` patching in production-like
RL, and on hacking generalizing to other misbehaviour; [METR](https://metr.org/blog/2025-06-05-recent-reward-hacking/)'s
on frontier models patching timers and reading the grader's answer off the call stack; a 1.5B Rust
model that met a "has tests" reward with tests that asserted nothing
([Oxen.ai](https://ghost.oxen.ai/training-a-rust-1-5b-coder-lm-with-reinforcement-learning-grpo/));
and Kevin's kernel writers falling back to the reference implementation
([Cognition](https://cognition.com/blog/kevin-32b)).
