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
- **Editing** (`research/experiments/editing/` and `indentation/`): twelve editing tasks with hidden
  tests in an indented and a braces form, a search-and-replace applier in strict and tolerant
  modes, and a mutation experiment on indentation and brace slips — see [[editing-robustness]].
- **Grammars** (`research/experiments/grammar/`): block-structure grammars checked with llguidance —
  see [[constrained-decoding]].
- **Sources** (`research/literature/`): every paper the decisions rest on, downloaded and converted,
  with each quoted number checked against it — see [[source-verification]].

## What is missing

1. **External tasks.** Translate HumanEval and MBPP the way
   [MultiPL-E](https://arxiv.org/abs/2208.08227) does for 18 languages. The work is a translator
   for signatures, docstring terminology, types and tests — prompt contents matter: removing
   doctests hurt Codex significantly. Add less contaminated tasks (LiveCodeBench v6) and
   multi-file tasks.
2. **Hidden tests for generation.** The pilot's programs pass their own tests; a task-level
   oracle written independently of the model is what pass@1 needs.
3. **Editing at scale.** The editing pilot used twelve short programs and three Claude models; the
   decision in adr:0005-significant-indentation needs long files, multi-turn agents and open
   models.
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
| 0 → 1 | parse ≥ 95% and no syntactic leakage for frontier models with the spec; data tests variant B against A (adr:0004-python-syntax-where-semantics-match) and settles indentation versus braces (adr:0005-significant-indentation) on at least 168 paired tasks |
| 1 → 2 | lotml pass@1 ≥ typed Python pass@1 on the same tasks; median rounds to green ≤ 2; tokens ≤ typed Python |
| 2 → 3 | calling lotml from Python and Python from lotml works; synthetic corpus validated by tests |
| 3 → 4 | the whole suite passes on the Python and C targets with the same result; ≤ 2× C on numeric benchmarks, with allocation-heavy ones reported separately |

The current pilot would score 93% parse (56 of 60) on the first criterion; with positional fields
in variants accepted, it would score 97%. The editing pilot passed 71 of 72 edits, which does not
yet meet the sample size above.
