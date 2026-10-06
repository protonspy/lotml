# Constrained decoding

The original study asks for a published formal grammar (EBNF and GBNF) so that "the model never
generates syntactically invalid code". The evidence keeps the grammar but shrinks its promise:
syntax is the smallest share of errors, a syntax-only mask can lower accuracy, constraining by
types raised compile rates but lowered functional correctness in an independent replication, and
the strongest gains come from constraining narrowly and letting the model reason freely. On
indentation, this project's own test settles the cost question. Every number taken from a paper in `research/literature/sources.json` is quoted in
`claims.json` there and checked against the paper ([[source-verification]]).

## What exists

- **Open engines:** llama.cpp's GBNF, [Outlines](https://arxiv.org/abs/2307.09702),
  [XGrammar](https://arxiv.org/abs/2411.15100), [SynCode](https://arxiv.org/abs/2403.01632),
  llguidance (about 50 µs per mask with a 128k vocabulary; used by OpenAI's grammar tools,
  llama.cpp, vLLM and SGLang).
  - XGrammar's "near-zero overhead" was measured end to end only on JSON; its one programming-
    language grammar, a Python DSL, ignores indentation and was measured only per mask, where it
    is several times slower than JSON.
  - SynCode reduced syntax errors by about 96% on average over three small 2023 models, with
    hand-trimmed grammars that leave out features such as Python's lambdas. It handles Python's
    indentation with a decoding-time tracker outside the grammar — and brace-delimited Go also
    needed handling outside the grammar, for its automatic semicolons.
- **Hosted APIs, as of October 2026:**
  - **OpenAI accepts a context-free grammar** in *custom tools*, in `lark` and `regex` syntax, via
    llguidance, on the GPT-5 models; the Lark subset rejects lookarounds, lazy quantifiers,
    terminal priorities, `%declare` and imports beyond `%import common`
    ([guide](https://developers.openai.com/api/docs/guides/function-calling)).
  - **Anthropic accepts JSON Schema only**, with no recursive schemas
    ([doc](https://platform.claude.com/docs/en/build-with-claude/structured-outputs)).
  - **Google Gemini accepts a subset of JSON Schema**, with no grammar.
  - **Fireworks accepts GBNF** on every model it serves.
- **Without masks:** checking each streamed prefix with the compiler and restarting at the first
  unrecoverable error needs no grammar support from the API, and was evaluated on Claude Opus and
  Gemini Flash among others ([Generative Compilation](https://arxiv.org/abs/2607.13921),
  [[semantic-compiler]]). Masking is for open models and OpenAI; prefix checking is for everyone.

## Does constraining help or hurt?

- **Format instructions, not constraints, caused the famous drop.** In
  [*Let Me Speak Freely?*](https://arxiv.org/abs/2408.02442), Claude 3 Haiku's GSM8K fell from 86.51
  to 23.44 with a JSON schema written into the prompt — not JSON mode, which the paper ran only for
  other models; Claude's JSON answers failed to parse 60.07% of the time. The one true grammar-
  constrained run (gpt-4o-mini Structured Outputs) lost 2.86 points on GSM8K and won on Last Letter.
- **Enforcement of a reasoning-friendly format helps a little.** In
  [JSONSchemaBench](https://arxiv.org/abs/2501.10868), the four open engines gained at most 3.7 points
  over the same JSON prompt unenforced (Llama 3.1 8B: 80.1 to 83.8 with Guidance); llama.cpp tied on
  one task.
- **Constrain narrowly, reason freely.** [CRANE](https://arxiv.org/abs/2502.09061) leaves reasoning
  unconstrained and applies the grammar only between delimiters: up to 10 points over the best
  baseline. A semantic prefix oracle used the same mixed mode to track full chain-of-thought within a
  few points while emitting 28–36 tokens instead of 1,100–2,000
  ([arXiv 2609.35425](https://arxiv.org/abs/2609.35425)).
- **A syntax-only mask can hurt even when it rejects nothing valid:** 38.4% against 44.4% free on a
  typed lambda calculus, 25.4% against 30.2% on an ML; only the typed mask recovered the loss, and
  only where the type system constrains the surface. Constraints helped weak and base models (a 2B
  base model from 0% to 50%) and taxed fluent ones (9B and 27B lost ground).
- **Distortion is real and not cheaply fixed.** [Grammar-Aligned Decoding](https://arxiv.org/abs/2405.21047)
  names the problem; its algorithm removes the bias only asymptotically, over thousands of samples of
  the same prompt, with no strong effect on downstream tasks.
- **An incomplete grammar can be a disaster — when the model wants what it forbids.** In
  [*The Alignment Problem in Constrained Code Generation*](https://arxiv.org/abs/2606.21619), a GBNF
  grammar for TOML that forbade only the optional space before `=` cut one model's exact match from
  62.5% to 1.9% (the "up to 97%"); grammars that forbade comments or dotted keys lost nothing. The
  [[python-leakage-pilot]]'s grammar rejected two valid programs until it was fixed.

## Types are worth more than syntax — for compiling, not for passing

[Mündler et al.](https://arxiv.org/abs/2504.09246) (PLDI 2025) constrained TypeScript by types: on
synthesis plus translation it cut 75.3% (HumanEval) and 52.1% (MBPP) of compilation errors, where a
perfect syntax constraint could cut at most 9.0% and 4.9%; repair rose by 37% relative pass@1. To make
it work they made TypeScript stricter — annotated parameters and returns, initialized or annotated
variables — and excluded features that block left-to-right typing.

The alignment paper replicated that decoder at scale (about 104,000 programs): type constraints
raised compile rates for weaker models but **lowered functional correctness in every configuration**
(Qwen-2.5-32B on HumanEval at temperature 0.1: 72.3% constrained against 82.3% free).
[Monitor-Guided Decoding](https://arxiv.org/abs/2306.10763) raised Java compilation by 13.18–24.69%,
relative. The lesson for lotml: a type system checkable on prefixes is worth having for the
[[semantic-compiler]]'s prefix checks and repair, and masking by types is a tool for weak open
models, not a default — see [[type-system]].

**Measured on lotml** (phase 2, `harness/results/masks.md`): three open models wrote 100 tasks of
the phase 1 sample twice, free in one greedy call and masked a line at a time, each line checked by
`lotml check --prefix` and drawn again when the program could no longer complete. Neither Ollama
nor OpenRouter lets a caller mask tokens, so the line is the unit refused — a coarse mask. It helped
the weakest model and no other: Llama 3.1 8B rose from 17% to 29% (12 tasks passed only masked, none
only free; McNemar p < 0.001), Qwen2.5-Coder 7B from 51% to 56% (5 against 0, p = 0.062), and
GLM-5.3-Flash went from 61% to 59% (6 against 8, p = 0.79). The masks cut programs that do not
check in every model (Llama 72 to 53) but turned few of them into passing ones once a model was
strong enough to write code that checks — the alignment paper's pattern, at the scale of a line.
Llama and GLM ran on OpenRouter from one pinned provider each, Qwen locally; each comparison is
within one model and one provider.

## Indentation needs a line-oriented, bounded grammar — not more decode time

`research/experiments/grammar/` tested lotml's block structure with llguidance 1.9.1 and the `o200k`
tokenizer, on the paired corpus reduced to its blocks:

- **The pilot's Lark grammar is refused**, for two features: the terminal priority on `**` and
  `%declare _INDENT _DEDENT`. With both replaced it compiles, but `INDENT` and `DEDENT` become
  literal tokens no real program contains.
- **A token-level grammar cannot enforce indentation** when it ignores spaces between tokens, as a
  full language grammar does: a newline terminal carrying exactly four spaces still accepted a line
  indented eight, because the extra spaces were ignored.
- **A line-oriented grammar bounded in depth works**: one copy of the block rules per nesting level,
  each level's lines matched whole with exactly that many spaces. It accepts all 12 programs,
  rejects a header without a body and a block deeper than its bound, and grows by 4 rules per level
  (20 rules at depth 4, 132 at depth 32).
- **It costs no decode time**: median mask computation was 19–23 µs per token for the bounded
  indentation grammars and 19 µs for the braces grammar — one timing run of about 2,400 tokens per
  grammar, which moves by a few microseconds between runs; read it as the same order of magnitude.

So significant indentation does not make the constrainer slower; it makes the grammar a generated
artifact — depth-bounded and line-oriented — rather than a hand-written one. That fits requirement
1 below anyway.

## The published grammar

`reference/grammar/` holds the three dialects, generated by `harness/lotml_harness/lang/dialects.py`
from the parser's own Lark source and tested on every change against every variant B program of the
paired corpus, the reference and the pilot: the EBNF is read back into a parser with an indenter, and
the llguidance Lark and the GBNF are compiled by llguidance and fed each program through the `o200k`
tokenizer. The two constrained dialects are line-oriented and bounded at eight block levels; they
reject indentation that is not four spaces per level, which the tolerant parser accepts, and require
whitespace only between two words, splitting rules such as the comparison operators so `x<y` stays
legal. Building them surfaced one engine constraint: llguidance's lexer decides a lexeme one byte
ahead, so no lexeme may run on into the next line's indentation — a line end is one lexeme per line,
and GBNF whitespace is a recursive rule so a converter cannot fuse `is` and `not` into one lexeme.

A fourth dialect is for editors, not for decoding: `reference/grammar/tree-sitter/`, a
tree-sitter grammar generated from the same source, with highlight queries whose keywords come
from it too. Three things had to be said explicitly that Lark implies. The layout tokens come
from a hand-written external scanner, zero-width, so comment lines stay in the tree. Every rule
associates to the right, because Lark's LALR parser settles each shift/reduce conflict by
shifting. A rule Lark inlines (`?rule`) is hidden, and renamed where the source also has a rule
of that hidden name. It is tested by tree-sitter's own CLI, which generates the parser and parses
every variant B program of the paired corpus and every reference example without an error.

## Requirements for the published grammar

1. **The grammar accepts exactly the compiler's language**, never less: it is generated from the
   same source as the parser and tested against the same corpus on every change. What it may never
   forbid is what models prefer to write.
2. **Three dialects:** an llguidance-compatible Lark grammar (no priorities, no `%declare`;
   line-oriented blocks generated to a depth bound such as 16), GBNF (llama.cpp, Fireworks) and EBNF
   (vLLM, XGrammar).
3. **Bounded repetition** and no chains of optionals — the GBNF README warns that `x? x? x?` can make
   sampling "extremely slow".
4. **Constrain the code, not the reasoning:** the grammar applies between delimiters, CRANE-style,
   and comments are allowed anywhere inside.
5. **Prefix checks before masks:** the compiler's `check --prefix` serves every model; type masks
   are offered for open models and measured in the [[evaluation-harness]] before anyone relies on
   them.
