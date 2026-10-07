# Training prior

A new language starts with a corpus of zero, and the model brings to it what it learned from
others. This page gathers the evidence on that risk — the original study's biggest — and the
measured ways around it. Reading the sources in full and adding the 2026 studies sharpened the
picture: a short syntax reference fixes most of the syntax, and what is left is implementation
fidelity and semantics, which is exactly where lotml departs from Python. The design rule that
follows is in [[lotml-syntax]]. Every number taken from a paper in `research/literature/sources.json` is quoted in
`claims.json` there and checked against the paper ([[source-verification]]).

## The size of the problem

- **Performance tracks the language's popularity.** [MultiPL-E](https://arxiv.org/abs/2208.08227)
  grouped 18 languages by TIOBE rank and GitHub share — not by training-data frequency, which is not
  public — and found differences between the groups for Codex, with exceptions (Lua does well).
- **Young languages start near zero.** [Giagnorio et al.](https://arxiv.org/abs/2606.16827) measured
  Gleam and MoonBit on McEval-Hard: 0.18–0.97% and 0.88–1.10% zero-shot. Few-shot and retrieval
  helped (GPT-4o on MoonBit: 0.88% zero-shot, 8.06% five-shot, 6.12% with retrieval); the best
  technique was instruction transfer onto a further-pretrained base model, reaching 26% on Gleam and
  33% on MoonBit.
- **Single-shot prompting with a spec fails on alien languages; agents do not.** In
  [EsoLang-Bench](https://arxiv.org/abs/2603.09678) the best documentation-only result was 6.2%, and
  11.2% needed interpreter feedback. On the same problems, agents with a reference card, local
  interpreter runs and up to three submissions averaged 86.9% (Opus 4.6) and 99.7% (GPT-5.4), but
  24.7% for Haiku 4.5 ([arXiv 2606.10933](https://arxiv.org/abs/2606.10933)). The strongest agents
  succeeded by writing generators in a familiar language — Python, JavaScript or Rust — for
  Brainfuck and Befunge: an adaptation, not a flight to Python.
- **A full spec does not stop forbidden constructs.** In [PyLang](https://arxiv.org/abs/2605.15607),
  a minimal language absent from pretraining, frontier models given the complete spec and an explicit
  list of constraints still wrote `for` loops, chained indexing and built-ins the language lacks.
  With idiom snippets Sonnet 4.5 solved 58% against 88% in Python; when it failed in PyLang after
  solving in Python, it had chosen the same algorithm 77% of the time — the gap is implementation
  fidelity, not reasoning. Fine-tuning drove syntax errors below 5% and left a gap that is not
  syntactic.
- **Familiar syntax with a new meaning is worse than new syntax.** Models predicting what programs
  do under supplied rules lost 40–70 points when familiar operators carried new meanings, much more
  than with novel symbols for the same rules, and chain-of-thought did not help
  ([arXiv 2510.03415](https://arxiv.org/abs/2510.03415)). It measures reading, not writing, but it is
  the closest measurement of lotml's case: Python's `=` and argument passing with copy semantics.
- **Paradigm priors persist.** Asked for Haskell, OCaml and Scala, GPT-5 wrote mutable variables,
  loops and in-place updates in 80–94% of outputs, and error rates were significantly higher in the
  purely functional languages ([FPEval](https://arxiv.org/abs/2601.02060)). lotml's immutability by
  default will meet the same imperative pull.
- **Without a familiar parent, output drifts.** In [SPEAC](https://arxiv.org/abs/2406.03636) no model
  produced UCLID5 that parsed in 660 attempts. The authors used full Python as the parent language and
  a Python subset as the child; the whole pipeline — child-language prompt, type repair, model
  hole-filling — reached 84.8% (GPT-3.5) and 72.7% (GPT-4) on 33 exercises.
- **Language confusion is real but not only toward Python.** In
  [Moumoula et al.](https://arxiv.org/abs/2503.13620) adherence ranged from 74.33% to 97.60% across ten
  models, and models drifted mostly to Python for only five of them (others to Java or JavaScript);
  the near-universal Python default was measured where no language was named. On a benchmark that
  names the language, nearly all models were above 99% — two datasets, not a controlled ablation.

## What lotml's own measurements showed

In the [[python-leakage-pilot]], three Claude models with a 1,830-token spec wrote no Python syntax
in 60 programs; what appeared, in the smallest model, were semantic habits. Executing the programs
found the semantics a Python reader would get wrong — variant A's `or` and `if x:` on `0` — and a
test that passes only under reference semantics. In the [[editing-robustness]] pilot, a braces form
of lotml drew the C family's `else if`. Every neighbour's prior leaks where lotml resembles it.

## Ways around it, in order of evidence

1. **A short syntax reference in the prompt.** On Cangjie, a 2,146-token grammar reference raised
   every model by 22–46 points over direct generation — GPT-5 from 4.8% to 50.4% — and beat retrieval
   over the language's public corpus, which left more than half of failures as foreign syntax
   ([CangjieBench](https://arxiv.org/abs/2603.14501)). Idiom snippets beat stated rules: in PyLang
   they added 24 points, and rules without snippets lowered one model's score. lotml's spec is already
   short (the pilot's is about 1,830 tokens); it should be made of examples.
2. **Documentation and a compiler in the loop — for strong models.** [Shen et al.](https://arxiv.org/abs/2602.06976)
   took Cangjie from 3.23% / 7.74% / 45.16% zero-shot to 63.23% / 73.55% / 81.94% with an agent that
   navigates documentation, searches, looks up types, executes code and runs public tests; one-time
   retrieval of the documentation alone already reached 40.00 / 46.45 / 59.35, and removing the
   verification tools cost as much as removing search. In CangjieBench, agent loops beat the syntax
   reference only on the strongest backbone (Codex on GPT-5: 66.9% and 77.0%), at 10–120 times its
   tokens; ten sampled answers scored by tests beat a matched agent run in every cell.
3. **A synthetic corpus from test-validated translation.** [MultiPL-T](https://arxiv.org/abs/2308.09895)
   translated Python with the low-resource model itself and kept what passed tests: Julia 21.1→35.2,
   OCaml 6.9→19.9, Racket 11.8→21.0 pass@1. It was measured only on languages already in pretraining,
   and its authors expect it not to work as is for a language the model never saw — lotml's case. The
   PyLang corpus was produced by Claude Opus in an agentic loop against the real interpreter, keeping
   30.4% of attempts. For lotml the translator is a rule-based transpiler plus a frontier model in a
   loop with the compiler ([[transpilation-strategy]]). Built in phase 2 on the 509 tasks whose
   canonical typed Python passes its hidden tests: the rules alone translated 310 (61%) that
   passed, and Claude Sonnet with the compiler the other 199 — 197 on its first answer, from the
   Python, the reference and what the rules wrote — so every task is in the corpus, each program
   with its tests (`harness/results/corpus.md`). The cases are small functions; a corpus that
   teaches a model lotml needs larger, more varied programs than these.
4. **RL with verifiable rewards.** [Agnostics](https://arxiv.org/abs/2508.04865) trained Qwen3-4B with an
   input/output verifier: Lua 11→23%, Julia 10→22%, OCaml 1→7% on its LiveCodeBench port. It needed a
   5,369-problem dataset, a prompt of language tips and a nonzero starting success rate. It did not
   improve Qwen3-1.7B or Llama-3.2-3B on its competition problems, though SmolLM3-3B, starting at
   1–2%, did improve; [[small-coder-training]] has the rest of the training evidence. Preference
   tuning helps a new language less than a familiar one, because it only reweights correct samples
   the model already produces.
5. **Continued pretraining, fine-tuning and instruction transfer** of open models (the Gleam and
   MoonBit numbers above; Mojo-Coder reached 66.4% with 6 million tokens and 3,200 instructions).
6. **Explicit identity.** Its own extension, the language's name in the prompt and at the top of the
   file: cheap, and the confusion data point the same way.
7. **The compiler recognizes the neighbours' habits** and returns the targeted fix — `raise` becomes
   `fail`, `items = []` followed by `append` becomes `var items = []`, `else if` becomes `elif` — as an
   automatically applicable fix. CangjieBench's authors recommend the same: a small, compiler-checked
   index of the constructs that break most often, naming option types and mutability.

## Consequences for the project

- The risk did not disappear when the syntax stopped leaking; it moved to implementation fidelity
  and semantics. Where lotml diverges from Python — immutability, value semantics, `T ! E` — the model
  will follow its prior unless the syntax marks the difference and the compiler catches it.
- Do not put Python in the prompt next to the lotml task: adding the Python solution to GPT-5's
  prompt cut Cangjie pass@1 from 64.0% to 44.5%, though agents with more turns recovered.
- After a syntax reference, Cangjie's failures shifted to API misuse (34–42%) and type or mutability
  errors (about 16%): the standard library's shape and the mutability diagnostics are the next lever.
- The Python→lotml translator is the main lever for open models; for closed models, the short,
  example-based spec plus the [[semantic-compiler]].
