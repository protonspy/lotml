# Constrained decoding

The original study asks for a published formal grammar (EBNF and GBNF) so that "the model never
generates syntactically invalid code". The evidence confirms the grammar's value but corrects its
reach: syntax is the smallest share of errors, only one hosted API accepts an arbitrary grammar,
and a grammar that rejects valid programs is worse than none.

## What exists

- **Open engines:** llama.cpp's GBNF, [Outlines](https://arxiv.org/abs/2307.09702),
  [XGrammar](https://arxiv.org/abs/2411.15100) (MLSys 2025, "near-zero overhead"),
  [SynCode](https://arxiv.org/abs/2403.01632) (removes 96.07% of syntax errors in Python and Go),
  llguidance (about 50 µs per token with a 128k vocabulary; used in OpenAI's Structured Outputs,
  llama.cpp, vLLM and SGLang). vLLM accepts EBNF grammars with several of these backends.
- **Hosted APIs, as of October 2026:**
  - **OpenAI accepts a context-free grammar** in *custom tools*, in `lark` and `regex` syntax
    (regex in the dialect of Rust's `regex` crate), via llguidance, on the GPT-5 models. The Lark
    subset does not accept lookarounds, lazy quantifiers, terminal priorities, `%declare` or
    imports beyond `%import common`, and the API may reject a grammar as "too complex" with no
    published numeric limit ([guide](https://developers.openai.com/api/docs/guides/function-calling)).
  - **Anthropic accepts JSON Schema only**, with no recursive schemas
    ([doc](https://platform.claude.com/docs/en/build-with-claude/structured-outputs)).
  - **Google Gemini accepts a subset of JSON Schema**, with no grammar.
  - **Fireworks accepts GBNF** on every model it serves.

Source code needs a recursive grammar: today, generation of lotml code can only be constrained on
open models, on OpenAI and on Fireworks — not on Claude or Gemini.

## Does constraining help or hurt?

- **Distortion:** grammar constraints "can distort the LLM's distribution";
  [Grammar-Aligned Decoding](https://arxiv.org/abs/2405.21047) (NeurIPS 2024) corrects it.
- **Formats that hurt reasoning:** in [*Let Me Speak Freely?*](https://arxiv.org/abs/2408.02442)
  (EMNLP 2024 Industry), GSM8K fell from 86.51 to 23.44 for Claude 3 Haiku in JSON mode; the cause
  given is that answers put the answer key before the reasoning key. The
  [rebuttal from .txt](https://blog.dottxt.ai/say-what-you-mean.html), with equivalent prompts,
  found the opposite (0.77 unconstrained, 0.78 constrained).
- **Constraining done well wins:** in [JSONSchemaBench](https://arxiv.org/abs/2501.10868), every
  engine beat free generation (GSM8K with Llama 3.1 8B: 80.1 free, 83.8 with Guidance), and
  [CRANE](https://arxiv.org/abs/2502.09061) (ICML 2025) added up to 10 points by including
  reasoning rules in the grammar.
- **An incomplete grammar is a disaster:** when the constrainer rejects valid programs,
  functional correctness drops "by up to 97%" and free generation wins
  ([*The Alignment Problem in Constrained Code Generation*](https://arxiv.org/abs/2606.21619),
  2026).

The [[python-leakage-pilot]] saw this at small scale: the pilot's grammar rejected two valid
programs (`**` and a negative literal in a pattern) until it was fixed.

## Types are worth more than syntax

Constraining by types cut 75.3% (HumanEval) and 52.1% (MBPP) of compilation errors in TypeScript;
constraining syntax alone would, in the ideal case, cut 9.0% and 4.8%
([Mündler et al.](https://arxiv.org/abs/2504.09246), PLDI 2025). Repair improved by 37%, at a cost
of 39–52% more decoding time in an unoptimized Python implementation.
[Monitor-Guided Decoding](https://arxiv.org/abs/2306.10763) (NeurIPS 2023) used a language server
during generation and raised the compilation rate by 19–25%. MoonBit does the same in its sampler.
This calls for a type system checkable on prefixes — see [[type-system]].

## Significant indentation makes the constrainer more expensive

SynCode needed extra lexer machinery for Python, and OpenAI's best practices ask for explicit
whitespace in the grammar. A language with significant indentation needs synthesized
`INDENT`/`DEDENT` tokens, which OpenAI's Lark subset does not offer (`%declare` is not accepted).
It is a concrete argument for explicit delimiters — see [[lotml-syntax]].

## Requirements for the published grammar

1. **The grammar accepts exactly the compiler's language**, never less: it is generated from the
   same source as the parser, or tested against the same corpus on every change.
2. **Three dialects:** an llguidance-compatible Lark subset (OpenAI), GBNF (llama.cpp, Fireworks)
   and EBNF (vLLM, XGrammar).
3. **Bounded repetition** and no chains of optionals — the GBNF README warns that `x? x? x?` can
   make sampling "extremely slow".
4. **Free comments** allowed anywhere, so the model can reason inside the constrained output.
5. **A "valid continuations and expected type at the cursor" API** in the compiler, for type
   constraints — the step that actually reduces errors.

The prototype in `research/pilot/check.py` is a Lark grammar of both variants, with indentation
through `Indenter`; it serves as the pilot's checker, not as the published grammar.
