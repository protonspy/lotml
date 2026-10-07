# Language design evidence

What was measured, by October 2026, about the properties of a programming language that help or
hurt a model writing it. [[languages-for-agents]] lists the languages; this page lists the
evidence, property by property, and what each finding means for lotml. Details, marks for what
was and was not checked, and every source are in `research/llm-landscape/languages.md`. None of
these numbers is yet in `research/literature/claims.json`, so none is machine-checked.

## Python's head start is the largest effect

- **Popularity predicts success.** On 3,011 LeetCode problems, DeepSeek-V3 scored 79.81% in
  Python against 24.31% in Erlang and 20.82% in Racket. The gap between mainstream and niche
  languages was 28.9–44.8 points across five models
  ([arXiv 2509.23261](https://arxiv.org/abs/2509.23261)).
- **On aligned tasks, Python leads** Multi-LCB with a mean pass@1 of 0.482, and Java and C++ sit
  near 0.44 ([arXiv 2606.20517](https://arxiv.org/abs/2606.20517)).
- **Models default to Python** even when they recommend something else: they implement in Python
  35.3% of the time but recommend it 10.7% of the time
  ([arXiv 2608.06041](https://arxiv.org/abs/2608.06041)).
- **Related languages transfer.** Language families predict transfer between languages
  ([arXiv 2512.19509](https://arxiv.org/abs/2512.19509)).

All of it supports adr:0004-python-syntax-where-semantics-match and
adr:0010-variant-b-and-indented-blocks-settled-by-the-phase-0-gate: a Python-shaped surface is how
a language without a corpus borrows Python's. [[training-prior]] has lotml's own measurements.

## Brand-new languages, measured against Python

VeraBench is the first cross-language measurement of languages with no training data. It has 60
problems, nine frontier models and one run each.

- Vera averaged 98.7%, Python 96.7% and TypeScript 99.7%.
- Across five models, Vera scored 98.2, AILANG 96.8 and Aver 92.4 against Python's 97.0.
- The benchmark is saturated: TypeScript reached 100% for eight of nine models. It has no
  significance tests, and it confounds design with training data
  ([aallan/vera-bench](https://github.com/aallan/vera-bench), MIT).

With a skill file and small problems, a frontier model is no longer held back by a language's
novelty. lotml's phase 1 gate found the same for Sonnet, and found the opposite for 7–8B models
([[evaluation-harness]]). Posing VeraBench's 60 problems in lotml would place it in that
comparison with a paired design.

## Static types pay through the checker

- **Untyped failures surface late.** On VeraBench, Python's failures were wrong answers at run
  time 13 times and compile rejections 3 times; TypeScript had one of each.
- **A checker contains a weaker model.** One developer compared 28 days in each language after
  moving to a weaker model. The ratio of commits that fixed earlier work rose 21.6 points in
  TypeScript and 38.4 in Python ([arXiv 2607.13080](https://arxiv.org/abs/2607.13080)). That is
  one developer and one repository.
- **Annotations alone have no consistent sign.** In SWE-PolyBench, TypeScript beat JavaScript
  under two harnesses and lost under a third
  ([arXiv 2504.08703](https://arxiv.org/abs/2504.08703)).
- **Full annotations cost agent time.** Python with `mypy --strict` took 1.6–1.7 times as long
  as plain Python on a small agent task, across 600 runs with every run passing
  ([mame/ai-coding-lang-bench](https://github.com/mame/ai-coding-lang-bench)).

For lotml, the checker is the lever, not the annotations, as [[semantic-compiler]] already
argues. lotml targets "no worse than typed Python", so the typed-against-untyped tax is worth
measuring in its own harness rather than assuming.

## Indentation

- **Patching is where indentation fails.** Across 13 models and about 195,000 full-function
  patches, Python's poor results were "largely due to indentation issues in the generated
  patches", with Gemini the exception
  ([arXiv 2506.03283](https://arxiv.org/abs/2506.03283)).
- **In fresh code it is a minor error class.** It was 11.3% of 151 errors in generated Python
  tests ([arXiv 2506.14297](https://arxiv.org/abs/2506.14297)).
- **lotml's own gate appears to be the only controlled comparison.** No published study compares
  one language written with indentation and with braces. adr:0010's 192 paired editing tasks
  ([[editing-robustness]]) fill that gap.

That one measured harm is what lotml's symbol-addressed edits, which re-indent to their target,
were built for (adr:0009-significant-indentation-with-symbol-addressed-edits).

## Errors as values

- **No study compares** result types with exceptions in code a model writes.
- **Exception handling is a stable share of bugs.** It is 11.6–16.75% of SonarQube's bug findings
  in LLM-written Java across five models ([arXiv 2508.14727](https://arxiv.org/abs/2508.14727)).
- **Unstated rules are not enforced.** Preconditions are enforced 0% of the time when unstated,
  and 23–41% of the time when stated in the prompt
  ([arXiv 2510.12047](https://arxiv.org/abs/2510.12047)).

adr:0002-errors-as-values makes the error path part of the signature, which is where that
evidence says it must live. Splitting lotml's phase 1 results into fallible and infallible tasks
would give the first published comparison.

## Verbosity

- **Denser syntax is no win.** Every token-density claim in the new languages is either
  unmeasured or contradicted by its own table: Sui is larger than Python, and Weft used 3.2–3.7
  times as many tokens.
- **More explicit structure can raise accuracy.** Continued pretraining on grammar rules beat
  plain tokens by 8–20 points on HumanEval at 1.3B–7B
  ([arXiv 2503.05507](https://arxiv.org/abs/2503.05507)).
- **Familiarity beats terseness on cost.** OCaml and Ruby wrote the same number of lines, but
  OCaml cost $0.58 and Ruby $0.36.

This agrees with [[token-cost]].

## The model writes something familiar, a compiler lowers it

- **Quasar.** A restricted Python subset, transpiled and repaired from static errors, reached
  89.2% execution on AgentDojo. Unrestricted Python reached 76.3%
  ([arXiv 2506.12202](https://arxiv.org/abs/2506.12202) v2).
- **LLMLift.** Python as an intermediate form, with a checker, lowered to four DSLs almost
  without failure ([arXiv 2406.03003](https://arxiv.org/abs/2406.03003)).
- **Dafny as the intermediate form cost points.** It was compiled to Python and passed 77% of
  HumanEval, against 86% for writing Python directly
  ([arXiv 2501.06283](https://arxiv.org/abs/2501.06283)).

A checked language with Python's shape is lotml's thesis. Quasar is the closest measured case of
it.

## If contracts are ever added

No ADR proposes contracts, but the evidence is clear about how they should look if one does:

- **Model success on verification languages ranks Dafny > Verus > Lean, consistently.**
  - 82%, 44% and 27% on the vericoding benchmark
    ([arXiv 2509.22908](https://arxiv.org/abs/2509.22908)).
  - 40.3%, 24.7% and 7.8% on the same 77 algorithms
    ([arXiv 2602.09464](https://arxiv.org/abs/2602.09464)).
- **Python's prior does not carry the proofs.** Nagini, contracts on Python, trails Dafny
  ([arXiv 2503.14183](https://arxiv.org/abs/2503.14183)).
- **Proof languages need more than a Python shape.** Contracts should be discharged by an SMT
  solver, with what it cannot decide becoming a run-time check, as Vera does. A proof-heavy
  design (Lean) scores far lower. Expect weak specifications to be gamed ([[reward-hacking]]).

## Diagnostics and packaging have converged

The new languages agree on several points:

- **Diagnostics:** stable codes, an `explain` command, a concrete fix and a reference into the
  specification, for Vera, zerolang, Codong and Almide.
- **The guide ships from the binary,** as a skill or `llms.txt` matched to the version: Aver's
  `agent-connect`, AILANG's `ailang prompt`, BAML's `agent install`.
- **The program is a queryable database:** zerolang's hash-guarded `query` and `patch`, BAML's
  `describe`, Aver's `context`.

lotml's coded diagnostics, `lotml init` and symbol-addressed edits already sit there
([[semantic-compiler]]). Zerolang adds one caveat worth keeping: JSON where an agent parses, not
by default everywhere.
