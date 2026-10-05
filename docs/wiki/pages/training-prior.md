# Training prior

A new language starts with a corpus of zero, and the model brings to it what it learned from
others. This page gathers the evidence on that risk — the original study's biggest — and the
measured ways around it. The design rule that follows from it is in [[lotml-syntax]]: the same
syntax only where the semantics are the same.

## The size of the problem

- **Performance tracks the language's frequency in training.**
  [MultiPL-E](https://arxiv.org/abs/2208.08227) (Cassano et al.) measured 18 languages besides
  Python and found a significant correlation with frequency, with exceptions (Lua does well).
  Static typing on its own "neither helps nor hinders".
- **Young languages start near zero.** [Giagnorio et al.](https://arxiv.org/abs/2606.16827)
  (IEEE TSE, 2026) measured Gleam and MoonBit on McEval-Hard: 0.40–0.97% and 0.88–1.10%
  zero-shot; 5-shot up to 1.32% and 5.46–12.20%; RAG barely better; continued pretraining 12.47%
  and 25.86%.
- **A spec in the prompt is not enough for very different languages.**
  [EsoLang-Bench](https://arxiv.org/abs/2603.09678) (2026) gave five frontier models the full
  spec of esoteric languages: the best result was 11.2%, against 100% on the same problems in
  Python, and three solved examples added only 0.8 points.
- **Agents flee to Python.** Frontier models write Python programs that generate the code in the
  target language ([arXiv 2606.10933](https://arxiv.org/abs/2606.10933)), and agents prototype
  in Python before writing OCaml ([Tokenmaxxing](https://arxiv.org/abs/2607.22807)).
- **Without a familiar "parent", the output drifts.** In [SPEAC](https://arxiv.org/abs/2406.03636),
  no model produced UCLID5 that parsed in 660 attempts, and the outputs looked like a different
  language each time. The authors chose a Python subset as the parent language and reached 84.8%
  parse.
- **Python is the attractor.** In [Moumoula et al.](https://arxiv.org/abs/2503.13620) (SANER
  2026), adherence to the requested language ranged from 74.33% to 97.60% across ten models,
  with "a strong default to Python"; naming the language explicitly raised adherence above 99%.
  In MojoBench (Raihan, Santos and Zampieri, NAACL Findings 2025), Claude 3.5 Sonnet answered in
  Python when asked for Mojo.

## What the pilot showed

In the [[python-leakage-pilot]], three Claude models with a 1,830-token spec in the prompt wrote
no Python syntax in 60 programs. What appeared, only in the smallest model, were semantic
habits: reassigning immutable locals, mutating lists without `var` and treating arguments as
references. For frontier models with the spec in context, the risk moves from syntax to
semantics.

## Ways around it, in order of evidence

1. **Queryable documentation plus a compiler in the loop.** On Cangjie,
   [Shen et al.](https://arxiv.org/abs/2602.06976) (2026) took generation from 3.23% / 7.74% /
   45.16% zero-shot (DeepSeek-V3.2 / Qwen3-Max / Claude Sonnet 4.5) to 63.23% / 73.55% / 81.94%
   by giving the agent documentation and compiler tools — the largest effect measured without
   training anything. It is the central argument for the [[semantic-compiler]].
2. **A synthetic corpus from test-validated translation.**
   [MultiPL-T](https://arxiv.org/abs/2308.09895) (OOPSLA 2024) translated Python functions with an
   LLM, kept only the translations that pass the tests and fine-tuned StarCoderBase-15B: Julia
   21.1→35.2, OCaml 6.9→19.9, Racket 11.8→21.0 pass@1. Llama 3 used the same technique. A
   Python-like language makes that translator easy — see [[transpilation-strategy]].
3. **RL with verifiable rewards.** [Agnostics](https://arxiv.org/abs/2508.04865) (ICLR 2026)
   trained Qwen3-4B with only an input/output verifier and a short per-language configuration:
   Lua 11→23%, Julia 10→22%, OCaml 1→7%. A compiler and tests are enough.
4. **Continued pretraining and fine-tuning** of open models (Gleam and MoonBit numbers above;
   Mojo-Coder reached 66.4% with 6 million tokens and 3,200 instructions).
5. **Explicit identity.** Its own file extension, the language's name in the prompt and at the
   top of the file: cheap and effective against confusion.
6. **The compiler recognizes Python habits** and returns the targeted fix — `raise` becomes
   `fail`, `items = []` followed by `append` becomes `var items = []` — as an automatically
   applicable fix.

## Consequences for the project

- The Python→lotml translator for generating a corpus stops being optional: it is the main lever
  for open models. For closed models, the lever is the short spec plus the semantic compiler.
- The short spec (original study's target: under 10,000 tokens) is feasible: the pilot's, which
  covers the core, is about 1,830.
- The semantics that diverge from Python — immutable by default, value semantics — are where the
  model goes wrong without noticing. They need dedicated diagnostics, not just documentation.
