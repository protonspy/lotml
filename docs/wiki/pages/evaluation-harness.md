# Evaluation harness

"Without a benchmark of its own, every syntax decision becomes opinion," says the original study,
which makes the harness the first deliverable. This page describes the revised harness: what
already exists in `research/`, what is missing, the metrics the evidence adds, the sample size a
decision requires and the roadmap's gates.

## What already exists

- **Paired corpus** (`research/tokens/`): 12 tasks in typed Python and in variants A and B, with
  a token counter over eight tokenizers — see [[token-cost]].
- **Pilot** (`research/pilot/`): two compact specs, ten tasks, a Lark grammar of both variants,
  syntactic and semantic leakage checkers, and the analysis that aggregates the runs — see
  [[python-leakage-pilot]].
- **Execution** (`research/experiments/transpiler/`): a research transpiler from variants A and B
  to a Python AST that runs `test` blocks under lotml's semantics or Python's, with a step budget
  for runaway loops and tracebacks that point at the `.x` line ([[transpilation-strategy]]).
- **Executor** (`harness/lotml_harness/`): the research transpiler grown to every construct of
  the language reference (`reference/lotml.md`) — `inout` arguments written back to the caller,
  sets, bit operators, trait defaults, positional variant fields — with `int` trapping outside
  i64, a step budget counted with `sys.monitoring`, tracebacks at the `.lotml` line, the
  mutability checker that rejects what a Python habit gets wrong, and a child process per
  program. It reproduces the research transpiler's outcomes on all of the pilot's programs.
- **Editing** (`research/experiments/editing/` and `indentation/`): twelve editing tasks with hidden
  tests in an indented and a braces form, a search-and-replace applier in strict and tolerant
  modes, and a mutation experiment on indentation and brace slips — see [[editing-robustness]].
- **Grammars** (`research/experiments/grammar/`): block-structure grammars checked with llguidance —
  see [[constrained-decoding]].
- **Sources** (`research/literature/`): every paper the decisions rest on, downloaded and converted,
  with each quoted number checked against it — see [[source-verification]].

- **Task set** (`harness/`): HumanEval and MBPP from MultiPL-E's typed originals and LiveCodeBench
  release v6, translated to lotml signatures and docstrings with hidden tests — 684 tasks and 5,522
  test cases, every HumanEval and MBPP task kept only when its canonical solution passes the
  translated tests (`harness/results/tasks.md`). A task is refused, never patched, when its types
  have no lotml form (`Any`, unions) or Python's result does not fit the signature (`240.0` for an
  `int`). MultiPL-E's license forbids training on it, so it feeds evaluation only.

## What is missing

1. **External tasks** — built, see the task set above. Translate HumanEval and MBPP the way
   [MultiPL-E](https://arxiv.org/abs/2208.08227) does for 18 languages. The work is a translator
   for signatures, docstring terminology, types and tests — prompt contents matter: removing
   doctests hurt Codex significantly. Add less contaminated tasks (LiveCodeBench v6) and
   multi-file tasks.
2. **Hidden tests for generation.** The pilot's programs pass their own tests; a task-level
   oracle written independently of the model is what pass@1 needs.
3. **Editing at scale** — built: 192 paired tasks on files of at least 150 lines, three turns
   with feedback, Claude and two open families ([[editing-robustness]]). Agents with tools editing
   several files are still missing.
4. **More model families.** Closed ones (Claude, GPT, Gemini) with the spec in the prompt; open
   ones (Qwen, Llama, DeepSeek), which should leak more and which are, with OpenAI, the only ones
   accepting grammar-based [[constrained-decoding]].
5. **The real name.** The pilot used the provisional name X; the harness uses lotml and the
   `.lotml` extension — the editing pilot already does. Naming the language goes with higher
   adherence in [Moumoula et al.](https://arxiv.org/abs/2503.13620), on a comparison across two
   benchmarks rather than a controlled one ([[training-prior]]).

## Metrics

The original study's — output tokens, context tokens, pass@1, repair rounds and Python leakage —
plus:

- **Loops on code that does not compile.** In [Tokenmaxxing](https://arxiv.org/abs/2607.22807)
  they are the mechanism behind a weak model's cost in an unfamiliar language; problem difficulty
  explains most of the variance, so the metric is reported per task, never pooled.
- **Semantic violations** — mutating an immutable, truthiness, an argument treated as a
  reference — which the parser does not catch and the pilot found.
- **Outcomes that depend on the semantics**: tests run under lotml's semantics and under
  Python's, as the research transpiler does; a test whose outcome changes marks code a Python
  reader would misjudge.
- **Rounds to green with structured versus terse diagnostics**, the question the literature has
  not settled ([[semantic-compiler]]).
- **Edit outcomes per block style**: applied, applied only with tolerant matching, rejected by the
  parser, silently changed, passing hidden tests; and edit size, since weaker models rewrite.

## Sample size

The pilot had ten tasks per cell; a difference of one or two failures there is noise. With paired
tasks (the same task in both variants) and McNemar's test, two-sided at 5% and 80% power.
`research/experiments/sample_size/` checks the normal approximation (Connor's formula,
n ≈ (1.96·√p + 0.84·√(p − δ²))² / δ², p the fraction of discordant pairs and δ the difference)
against the exact power of the exact test the harness would run, and against a 20,000-trial
simulation:

| difference to detect | discordant pairs | Connor's n | its exact power | paired tasks needed |
| --- | ---: | ---: | ---: | ---: |
| 10 points of pass@1 | 20% | 155 | 0.76 | 168 |
| 5 points of pass@1 | 10% | 312 | 0.76 | 337 |
| 10 points of pass@1 | 30% | 234 | 0.78 | 249 |
| 5 points of pass@1 | 20% | 626 | 0.78 | 658 |
| 20 points of pass@1 | 40% | 77 | 0.76 | 84 |

The approximation is 5–9% short: at Connor's n the exact test has 76–78% power, not 80%.
"Paired tasks needed" is the smallest n from which the exact power stays at 80% or more, since the
exact test's power is a sawtooth in n. Settling a syntax question takes a few hundred tasks per
comparison, independent ones — repeated samples of the same task are not independent pairs.

## Gates

Each gate is a measured criterion; if it does not pass, the next phase does not start and the
syntax is revisited ([[requirements-and-roadmap]]):

| gate | criterion |
| --- | --- |
| 0 → 1 | parse ≥ 95% and no syntactic leakage for frontier models with the spec; data tests variant B against A (adr:0004-python-syntax-where-semantics-match) and settles indentation versus braces (adr:0009-significant-indentation-with-symbol-addressed-edits) on at least 168 paired tasks |
| 1 → 2 | lotml pass@1 ≥ typed Python pass@1 on the same tasks; median rounds to green ≤ 2; tokens ≤ typed Python |
| 2 → 3 | calling lotml from Python and Python from lotml works; synthetic corpus validated by tests |
| 3 → 4 | the whole suite passes on the Python and C targets with the same result; ≤ 2× C on numeric benchmarks, with allocation-heavy ones reported separately |

The phase 0 gate passed (`harness/results/gate-0.md`, from `python -m
lotml_harness.experiments.gate`): Sonnet parsed all 350 of its variant B answers with no leaked
construct; no model did significantly worse in variant B than in A (pooled, 60 tasks solved only
in B against 26 only in A) nor in the indented form than with braces (pooled, 23 against 15), each
on 192 to 200 paired tasks. "Not worse" is read as no model significantly worse (exact McNemar,
5%) and the pooled discordant pairs not favouring the alternative. The decisions it settles are
adr:0010-variant-b-and-indented-blocks-settled-by-the-phase-0-gate.
