# Compiler-embedded model

A small language model specialised to one language and shipped inside its compiler or toolchain, to
locate and repair errors, explain diagnostics and suggest improvements. lotml does not have one:
its [[semantic-compiler]] answers with hand-written diagnostics and machine-applicable fixes, and
training models is outside the roadmap (`plans/lotml-roadmap.md`, out of scope). This page records what has
been tried elsewhere, so that the separate initiative starts from evidence. Every number taken from
a paper in `research/literature/sources.json` is quoted in `claims.json` there and checked against
the paper ([[source-verification]]); the few sources with no open PDF are cited by link, without
numbers.

## Small models repair; they do not write

- **A 0.5B fixer behind a validator.** SLMFix ([arXiv 2511.19422](https://arxiv.org/abs/2511.19422),
  preprint) calls Qwen2.5-Coder 0.5B only when the static validator reports errors, with the
  program, the prompt and the validator's messages; its reinforcement-learning reward averages the
  validator's verdict with AST similarity to a reference, and its data starts from 20 hand-written
  pairs per language, each sampled 10 times from five strong models. Behind Qwen2.5-Coder 7B it
  raised validator pass rates to 96.92% (Ansible) and 75.48% (Lean), against 59.56% and 15.96% for
  the 7B model alone and 73.83% and 7.31% for the 7B model fine-tuned with LoRA, adding 7.35 ms per
  sample — served by vLLM on the A40 GPUs its experiments ran on, not on a CPU.
- **The same small model cannot author.** Trained with the same recipe to write Ansible directly,
  the 0.5B model passed the validator 48.83% of the time, against 94.30% as a fixer of a larger
  model's drafts — the fixer starts from those drafts, so this measures the division of labour,
  not the small model alone. The authors say only that their RL method "does not work well" on
  direct generation, possibly from the small model's limited capability, on this one Ansible
  ablation. Cite the pair with that caveat.
- **It fixes syntax, not meaning.** On Spider's SQL test suites SLMFix left functional correctness
  where it was (0.702 against 0.699), below fine-tuning and in-context examples (0.716), and Lean's
  CodeBERTScore fell from 0.7320 to 0.6598. Its authors list the dependency on an efficient
  syntactic checker as a limitation; for Ansible they had to build one, and lotml already has it.
- **60M parameters for one formula language.** FLAME ([arXiv 2301.13779](https://arxiv.org/abs/2301.13779),
  AAAI 2024), T5-small trained only on Excel formulas, beat Codex-Davinci (175B), Codex-Cushman
  (12B) and CodeT5 (220M) in 10 of 14 repair and completion settings. Read per benchmark it is a
  tie: on 273 real forum formulas fine-tuned FLAME scored 0.76 top-1, level with few-shot Davinci
  and behind fine-tuned Cushman at 0.79; a 16M variant reached 0.73. Its repair data is synthetic —
  200K well-formed formulas corrupted by noise operators — and deduplicating the corpus per workbook
  rather than globally was worth 8 points on fine-tuned repair.
- **Size and domain pre-training add little once fine-tuned.** For Ansible YAML
  ([arXiv 2305.02783](https://arxiv.org/abs/2305.02783)), a 350M model fine-tuned reached BLEU 66.67
  against 50.4 for few-shot Codex — not the same footing: few-shot, Codex still beat the 350M model
  (50.40 against 46.58). Ansible pre-training added about a point over plain CodeGen (66.67 against
  66.03), and a 2.7B model did no better than the 350M one (65.25 against 66.03).
- **A good explanation is not a good fix in a language the model does not know.** Distilling
  GPT-3.5's repairs into 7B students from about 400 test-verified examples per language
  ([arXiv 2406.14867](https://arxiv.org/abs/2406.14867)) helped Perl, Go and Swift, and distilling
  the code as well as the rationale added nothing for JavaScript or Java; on the low-resource languages, models given only the teacher's rationale
  wrote a rationale GPT-4 judged correct 91.1% of the time and correct code 10.0% of the time. Its "low-resource" languages are still 0.1% (Perl)
  and 0.32% (Go) of a pretraining corpus; lotml is none ([[training-prior]]).

## The grammar enumerates; the model ranks

- **LaMirage** ([arXiv 2207.11765](https://arxiv.org/abs/2207.11765)) generates a repair engine from
  an annotated grammar plus domain rules: symbolic code produces every candidate, and neural models
  only localize the error and rank. Its top candidate matched the fix for 174 of 200 Excel formulas
  and 170 of 200 Power Fx formulas, against 147 and 106 for Codex-Edit, in a median 32.1 ms and
  134.4 ms against 5806.6 ms and 6417.6 ms for Codex-Edit, network time to its API included. Enumerating with CodeBERT or Codex instead dropped Excel
  to 135 and 128; removing the neural ranker cost nothing on Excel (173.9) and 19 formulas on Power
  Fx (150.9). Moving to Power Fx, whose public corpus is much smaller, cost LaMirage 4 repairs and
  Codex-Edit 41.
- **Its price is data or hand work.** The ranker needs at least tens of thousands of well-formed
  programs, and without them the engine falls back to symbolic ranking; the grammar's annotations
  and the domain strategies are a one-time manual cost.
- **Classification instead of generation.** SynShine ([arXiv 2104.14671](https://arxiv.org/abs/2104.14671))
  lets javac find the faulty line and a RoBERTa-base-sized model choose among 154 edit commands,
  reaching 74.89% exact match on single-token Java errors against 56.89–63.25% for earlier
  systems, in 0.88 s on average on a CPU with a 1.765 GB server. It falls to 29.4% and 14.4% when 2
  or 3 edits are needed, and it trained on about 540K real novice fixes — labels lotml does not
  have.
- **Seq2Parse** ([OOPSLA 2022](https://doi.org/10.1145/3563330), no open PDF) does the same for
  Python parse errors: a classifier picks a few error-correcting grammar rules and an error-correcting
  parser builds the repair, so the output always parses.

lotml's `check --fix` is already the symbolic half for the neighbours' habits; a model that ranks
candidates the grammar enumerates is the smallest model that adds anything to it.

## The compiler is the critic, and the source of data

- **Real errors beat synthetic ones.** Break-It-Fix-It ([arXiv 2106.06600](https://arxiv.org/abs/2106.06600))
  learns repair with no labels: the parser or compiler accepts or rejects the fixer's outputs on real
  broken code, and a learned breaker makes realistic errors. Training longer on random corruption
  alone stayed at 62.7% against 62.0% on GitHub-Python; with only 10% of the 23K real broken
  snippets self-training reached 78.5%, and the full method 90.5%. Removing the critic's check
  dropped it to 84.0%. The model is a 4-layer Transformer with hidden size 256 — and "repaired"
  means it parses within an edit distance under 5 tokens, not that it means the same.
- **Corruption shaped like compiler errors, and the message as input.** DrRepair
  ([arXiv 2005.10636](https://arxiv.org/abs/2005.10636)) pre-trains on about 1.5M examples made by
  corrupting 310K compiling programs, no human labels. Its DrPerturb applies one to five edits per
  program from four error-shaped modules — syntax, identifier type, identifier typo, keyword —
  sampled by how often each error occurs; it repaired 62.5% of DeepFix against 49.4% for random
  token dropout, which made more distinct errors (170 against 156). The weights alone were never
  ablated: the authors credit the error distribution. Without the compiler
  message the model looked the same on synthetic data and fell to 34.0% on the real test set — it
  had learned the corruptions. It predicts the line to fix itself rather than trusting the reported
  one; its motivating example has the error reported on line 9 and the fix on line 5.
- **Mutate clean code, compile it, keep the messages.** HDLdebugger ([arXiv 2403.11671](https://arxiv.org/abs/2403.11671),
  KDD 2024) built 92,143 (broken code, compiler messages, fix) triples this way and fine-tuned
  CodeLlama-13b: direct prompting fixed 4.01%, retrieval 15.05%, fine-tuning 70.56% and the full
  system 81.93% — on a test split from the same generator, not real bugs. In deployment its authors
  found that compiling is not enough.
- **Seeded bugs train a detector, not a reviewer.** DeepBugs ([arXiv 1805.11683](https://arxiv.org/abs/1805.11683))
  trains a one-hidden-layer network of 200 units on bugs seeded by AST transformations and predicts
  in under 20 ms per file. Its 89.06–94.70% accuracy is on seeded bugs; on real code 68% of the top
  150 warnings pointed to a real problem, and the default threshold emits one warning per 196 lines.

What lotml already has for this: the answers the compiler refused in the phase 1 gate — the real
errors Break-It-Fix-It found decisive — and the programs that check clean, to mutate
([[evaluation-harness]], [[semantic-compiler]]).

## Explanations: accurate is not helpful

- **A model inside a compiler, unchecked.** dcc --help ([arXiv 2308.11873](https://arxiv.org/abs/2308.11873),
  SIGCSE 2024) prompts gpt-3.5-turbo with the source, DCC's enhanced message and the line; 2,565
  students used it over 64,000 times in ten weeks. In 400 sampled explanations it was conceptually
  accurate in 90% of compile-time and 75% of run-time cases, free of any inaccuracy in 78% and 53%,
  and 48% and 49% contained code despite a prompt forbidding it. Nothing checks the output before it
  is shown, and the paper did not measure whether students were helped.
- **A 4B model can be distilled to near the teacher.** Fine-tuned on 40,000 DCC Help errors with
  GPT-4.1 writing the answers ([arXiv 2507.05305](https://arxiv.org/abs/2507.05305), preprint),
  Qwen3-4B's expert-judged correctness rose from 0.74 to 0.89, within 0.10 of GPT-4.1 on every
  expert metric — while experts still preferred GPT-4.1 in head-to-head comparisons, the 4B model
  winning 0.34 (compile-time) and 0.36 (run-time) of them. The teacher bounds it, and lotml has
  neither the 40,000 logged errors nor a teacher that knows the language.
- **Unvalidated, about half are right.** Over 81 outputs on short Python snippets the authors wrote
  themselves, Codex explained the error correctly for 48% of inputs and fixed it for 33%
  ([arXiv 2210.11630](https://arxiv.org/abs/2210.11630)), wrong answers sounding as confident as
  right ones. PyFiXV ([arXiv 2302.04662](https://arxiv.org/abs/2302.04662)) shows the feedback only
  if a second model, playing the student, reproduces the fix from the explanation: precision rose
  from 38.9 to 76.0 on TigerJython and from 55.2 to 72.4 on Codeforces, while coverage fell from
  92.5 to 31.2 and from 98.8 to 64.2 — staying silent is the price.
- **Better-rated is not faster.** With 106 students fixing six C programs
  ([arXiv 2409.18661](https://arxiv.org/abs/2409.18661), UKICER 2024), GPT-4's explanations — every
  one correct, with the full fixed code — beat GCC's message on time-to-fix in 1 task of 6, and
  were slower on one; students still rated them higher. The hand-written messages that did best
  follow rustc's Error/Help/Note layout. With 103 participants
  ([arXiv 2608.20896](https://arxiv.org/abs/2608.20896)), llama-3.1-8B's rewrites were rated clearer
  and changed no measure of fixing significantly; the clearest gain, descriptive only, came on an
  error whose interpreter message points to the wrong line. GPT-4 hints in a C# course
  ([arXiv 2403.12737](https://arxiv.org/abs/2403.12737), ITiCSE 2024) did not consistently reduce
  errors while they were on; once switched off, the hinted group made fewer errors on 5 of the 6
  most frequent kinds, read off charts without a significance test.
- **The positive trial.** A randomized trial in an online introductory course reports students
  repeating errors less often with GPT-written messages ([Wang et al.](https://doi.org/10.1145/3626252.3630764),
  SIGCSE 2024; no open PDF).

lotml's diagnostics already follow the model that won ([[semantic-compiler]]). Every study above
has students as the reader; none has an agent.

## Learned lints: the filter is the product

- **Passing the checker is not passing review.** CORE ([arXiv 2309.12938](https://arxiv.org/abs/2309.12938))
  has GPT-3.5 propose revisions for CodeQL warnings, re-runs CodeQL to drop failing ones, and has
  GPT-4 rank the rest. Reviewers rejected 1321 of 2397 revisions that passed CodeQL (55.11%). Its
  headline 59.2% is the share of a 520-file user-study subset whose ranked revision a reviewer
  accepted.
- **Precision was reached by suppression.** Google's AutoCommenter ([arXiv 2405.13565](https://arxiv.org/abs/2405.13565),
  AIware 2024) flags best-practice violations mined from review comments. Its useful ratio
  plateaued at 54% against an 80% target, and reached it by suppressing non-actionable practices
  (54% to 66%) and rewriting summaries by hand. Beam search took 2 seconds, too slow for the IDE,
  which uses greedy decoding. When a Python practice changed in 2022, the model kept giving the old
  advice and was patched with regular expressions. An A/B test over about half of Google's
  developers found no significant change in review time; in its authors' judgement, 33 of the 50
  most frequent practices are beyond what a linter detects.
- **Surprise is a weak signal.** In Ray et al. ([arXiv 1506.01159](https://arxiv.org/abs/1506.01159))
  buggy lines are more entropic under an n-gram model, but raw entropy is only 17% better than
  random at 5% of lines inspected, a copy-paste bug looks natural, and the authors concede that a
  surprising line says less than a warning that names what is wrong. It also needs a corpus.

A practice the compiler can state as a rule is a diagnostic with its own code, as the neighbours'
habits already are; a model earns a place only for what no rule can express, behind a per-practice
switch.

## Inside the toolchain

- **In-process, checked before shown.** Lean Copilot ([arXiv 2404.12534](https://arxiv.org/abs/2404.12534))
  runs its model inside Lean through the FFI, locally with CTranslate2 or on a server, and drops
  every suggested tactic that fails against the current goals before the user sees it. Replaying
  textbook proofs, it left 2.08 tactics per theorem for the human against 3.86 for aesop and
  automated 74.2% of steps against 40.1% — a replay, not a user study; running without a GPU is a
  stated goal, not a measurement. Its authors warn that a static model goes stale as a project adds
  premises.
- **A shipped local model.** JetBrains' Full Line Code Completion ([arXiv 2405.08704](https://arxiv.org/abs/2405.08704))
  ships 100M-parameter models quantized to INT4 in llama.cpp, from almost 400 MB to slightly over
  100 MB, within a memory budget of 20–40% of the IDE's own; a suggestion takes 150 ms on average
  to show, on the CPU. Dropping suggestions that fail the IDE's inspections cost 1% of the valuable
  ones, and knowledge distillation gave nothing at that size.
- **The verifier decides.** Eiffel-tools ([arXiv 2609.03086](https://arxiv.org/abs/2609.03086)) is a
  language server that runs the AutoProof verifier on every model suggestion and applies none that
  fails. Allowed 10 attempts on 539 bugs, three models fixed 76–95%; withholding the verifier's
  error from the prompt still left 76–90%, and the full prompt added 6.4 points on average.
- **The diagnostic's quality carries the fix.** RTLFixer ([arXiv 2311.16543](https://arxiv.org/abs/2311.16543),
  DAC 2024) retrieves hand-written guidance keyed by the compiler's error — 30 entries for iverilog,
  45 for Quartus — and GPT-3.5 fixed 98.5% of compile errors with Quartus's messages against 82.0%
  with iverilog's terser ones; without the guidance, whose two sets differ, the gap was 79.9%
  against 73.1%. With GPT-4, iterating added about 1% over one shot. On RTLLM it lifted
  syntax success from 73% to 93% and simulation pass@1 only from 11% to 16%. In RING
  ([arXiv 2208.11640](https://arxiv.org/abs/2208.11640), AAAI 2023), adding the compiler's message to
  a zero-shot prompt raised JavaScript repair from 0.19 to 0.35; on PowerShell, which the authors
  presume scarce in the model's training, it reached 0.10.

## What it means for lotml

1. **The compiler gates everything — necessary, not sufficient.** The systems that held up show
   only output their checker accepts — Lean Copilot, JetBrains' inspections, Eiffel-tools,
   Break-It-Fix-It's critic, PyFiXV's validation; the ones that showed unchecked text are where
   prompt rules leaked and precision sat near half. Yet CORE's reviewers rejected 55.11% of
   revisions that passed CodeQL. A model's fix in lotml would be a structured patch that `check`
   re-runs, one that does not check is never shown, and one that checks is still only a proposal.
2. **Repair and ranking, not authoring or teaching.** Small models fix syntax and leave meaning
   where it was; better-rated explanations did not fix faster.
3. **The reader is an agent, and it matters which.** In the phase 1 gate the frontier models fixed
   nearly everything the compiler refused from the diagnostics alone, while the 7–8B models wrote
   syntax lotml does not have and mostly did not recover ([[evaluation-harness]]); GPT-4 fixed RTL
   syntax about as well in one shot as with iteration, and Eiffel's models fixed most bugs without
   the verifier's message. A frontier agent gains at most a cheaper `check --fix`; a small agent is
   SLMFix's own setting — a 7B generator with a 0.5B fixer behind it — and is where a fixer could
   move pass@1.
4. **Data comes from the compiler.** The refused answers, mutations of programs that check, and the
   checker as critic and reward; a ranker needs tens of thousands of well-formed programs, and
   lotml has no such trainable corpus. The 509 programs translated from MultiPL-E may not be
   trained on, and the rebuild from the HumanEval and MBPP originals (`plans/seed-corpus-rebuild.md`)
   stops at those two sources — hundreds of programs; [[transpilation-strategy]] would have to grow
   past them.
5. **A trained model goes stale with its language.** Lean Copilot and AutoCommenter both hit it;
   a rule changes with the compiler, a model needs retraining, and lotml is still changing.
6. **Open:** no study measures an agent consuming a small model's repairs. The harness can: the same
   tasks with `check` alone and with `check` plus a fixer, scored on pass@1 and tokens
   ([[evaluation-harness]]).
7. **How such a model is trained** — distillation, reinforcement learning against the compiler, and
   the bugs to train on — is [[small-coder-training]] and [[repair-training]].

## Not machine-checked

Earlier and industrial work with no open PDF, cited for what it built: language models beside the
compiler to locate syntax errors — n-grams for Java ([Campbell et al., MSR 2014](https://doi.org/10.1145/2597073.2597102))
and n-gram and LSTM models proposing token fixes ([Santos et al., SANER 2018](https://doi.org/10.1109/SANER.2018.8330219));
Google's repair of build errors from compiler diagnostics ([DeepDelta, FSE 2019](https://doi.org/10.1145/3338906.3340455);
[Graph2Diff, 2020](https://doi.org/10.1145/3387940.3392181)) and its fixes shown in the IDE after
safety filters ([Google Research, 2024](https://research.google/blog/safely-repairing-broken-builds-with-ml/)).
