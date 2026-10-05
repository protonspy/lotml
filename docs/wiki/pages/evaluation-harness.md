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

## What is missing

1. **Execution.** Without a minimal transpiler to Python there is no pass@1, only parsing and
   static rules ([[transpilation-strategy]]).
2. **External tasks.** Translate HumanEval and MBPP the way [MultiPL-E](https://arxiv.org/abs/2208.08227)
   does for 18 languages (the work is writing the test translator for lotml), and add less
   contaminated tasks such as LiveCodeBench v6's and multi-file tasks.
3. **Editing tasks.** Apply changes to existing lotml code by search-and-replace or diff, to
   decide indentation versus delimiters ([[lotml-syntax]]).
4. **More model families.** Closed ones (Claude, GPT, Gemini) with the spec in the prompt; open
   ones (Qwen, Llama, DeepSeek), which should leak more and which are, with OpenAI, the only ones
   accepting grammar-based [[constrained-decoding]].
5. **The real name.** The pilot used the provisional name X; the harness uses lotml and the
   `.lotml` extension, because naming the language explicitly raised adherence above 99% in
   [Moumoula et al.](https://arxiv.org/abs/2503.13620) ([[training-prior]]).

## Metrics

The original study's — output tokens, context tokens, pass@1, repair rounds and Python leakage —
plus:

- **Loops on code that does not compile**, which dominate agentic cost
  ([Tokenmaxxing](https://arxiv.org/abs/2607.22807)).
- **Semantic violations** — mutating an immutable, truthiness, an argument treated as a
  reference — which the parser does not catch and the pilot found.
- **Rounds to green with structured versus terse diagnostics**, the question the literature has
  not answered ([[semantic-compiler]]).
- **Rate of successfully applied edits**, per block style.

## Sample size

The pilot had ten tasks per cell; a difference of one or two failures there is noise. With paired
tasks (the same task in both variants) and McNemar's test, at 5% significance and 80% power:

| difference to detect | discordant pairs | paired tasks |
| --- | --- | ---: |
| 10 points of pass@1 | 20% | about 155 |
| 5 points of pass@1 | 10% | about 310 |

Estimated with the normal approximation, n ≈ (1.96·√p + 0.84·√(p − δ²))² / δ², where p is the
fraction of discordant pairs and δ the difference. Settling a syntax question takes a few hundred
tasks per comparison and more than one sample per task — not the few dozen a HumanEval-style
benchmark has on its own.

## Gates

Each gate is a measured criterion; if it does not pass, the next phase does not start and the
syntax is revisited ([[requirements-and-roadmap]]):

| gate | criterion |
| --- | --- |
| 0 → 1 | parse ≥ 95% and no syntactic leakage for frontier models with the spec; data confirms variant B (adr:0004-python-syntax-where-semantics-match) and settles indentation versus braces (adr:0005-significant-indentation) |
| 1 → 2 | lotml pass@1 ≥ typed Python pass@1 on the same tasks; median rounds to green ≤ 2; tokens ≤ typed Python |
| 2 → 3 | calling lotml from Python and Python from lotml works; synthetic corpus validated by tests |
| 3 → 4 | the whole suite passes on the Python and C targets with the same result; ≤ 2× C on numeric benchmarks, with allocation-heavy ones reported separately |

The current pilot would score 93% parse (56 of 60) on the first criterion; with positional fields
in variants accepted, it would score 97%.
