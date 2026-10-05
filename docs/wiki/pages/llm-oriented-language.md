# LLM-oriented language

lotml is a programming language designed to be written by LLMs and reviewed by people:
Python-like syntax, static types, errors as values, value semantics and a compiler built as an
agent's tool. This is the study's entry page — what was verified, what changed relative to the
original proposal of 2026-10-05, and where each part lives.

## Conclusions

1. **The thesis holds, but the bottleneck is not syntax.** 94% of LLM compile errors in typed
   code are type errors; syntax is about 6%. What decides is the [[semantic-compiler]] and a
   [[type-system]] checkable while the code is being written.
2. **The token saving from syntax is about 10%**, not the original target's 20%, and in agentic
   use it weighs less than one avoided repair round ([[token-cost]]).
3. **The corpus risk moved.** With the spec in the prompt, three Claude models wrote no Python
   syntax in 60 programs; the errors were semantic habits — mutating immutables, treating
   arguments as references ([[python-leakage-pilot]]).
4. **Where the semantics are Python's, the syntax should be Python's.** Variant B costs 0.8
   percentage points more tokens and removes two traps the parser cannot catch ([[lotml-syntax]]).
5. **Value semantics with reference counting is the right choice**, as long as the language has
   explicit parameter conventions — the gap the pilot exposed — and avoids atomic counting
   ([[memory-model]]).

## The original study, claim by claim

| original study's claim | verdict | where |
| --- | --- | --- |
| "less verbose" means fewer tokens, not fewer characters | confirmed | [[token-cost]] |
| rare symbols and abbreviations cost more tokens | **corrected:** APL glyphs cost 2–4 tokens; `cnt`, `idx`, `msg` cost 1, like the full word | [[token-cost]] |
| the gain comes from removing boilerplate, not shortening names | confirmed: a record saves 7 tokens, `fn` versus `def` saves 0 | [[token-cost]] |
| `str?` costs 1 token instead of 3–4 | **refuted:** it saves 1 against both `Optional[str]` and `str \| None` | [[token-cost]] |
| the proposed version has less than half the lines | true in lines (−62%); in tokens, −12% to −16% | [[token-cost]] |
| programs ≥ 20% smaller in tokens than Python | **refuted:** 9–11% against typed Python on eight tokenizers | [[token-cost]] |
| mandatory signatures and local inference help most | confirmed, with an adjustment: inference checkable on prefixes | [[type-system]] |
| the compiler is the most important half | confirmed and reinforced: docs plus compiler took Cangjie from 3–45% to 63–82% | [[semantic-compiler]] |
| fixes with numeric confidence | **corrected:** applicability levels, as in rustc | [[semantic-compiler]] |
| the digest brings more gain than trimming the syntax | partly: it improves localization and cost; generation still needs the bodies | [[semantic-compiler]] |
| a published grammar prevents invalid code | **corrected:** it only covers syntax, only OpenAI and open models accept it, and an incomplete grammar makes generation worse | [[constrained-decoding]] |
| risk number one is the lack of a corpus | **corrected:** for frontier models with the spec, leakage is semantic, not syntactic | [[training-prior]] |
| no language has been designed for LLMs | **outdated:** BAML, MoonBit, Pel, NanoLang, Zero, Quasar — none with measured evidence | [[languages-for-agents]] |
| `use` avoids implicit relative imports | **corrected:** Python 3 no longer has implicit relative imports | [[lotml-syntax]] |
| LLMs get Rust lifetimes wrong a lot | **corrected:** ownership is 16.7% of errors; names and traits dominate | [[memory-model]] |
| mutable values with ARC and elision | confirmed, with non-atomic counting and `inout` conventions | [[memory-model]] |
| configurable overflow in release | **corrected:** one semantics in every build and on every target | [[type-system]] |
| colorless concurrency eliminates the forgotten `await` | plausible, not measured | [[colorless-concurrency]] |
| transpile to Python, then C, then native | confirmed, with a minimal transpiler already in phase 0 | [[transpilation-strategy]] |

## Pages

**What was measured**
- [[token-cost]] — eight tokenizers, a 12-task paired corpus, isolated constructs.
- [[python-leakage-pilot]] — three models, two variants, 60 programs.

**The context**
- [[training-prior]] — why new languages start near zero, and how to get around it.
- [[languages-for-agents]] — what already exists and what none of it measured.

**The design**
- [[lotml-syntax]] — the rule, variants A and B, the gaps, indentation.
- [[type-system]] — types checkable on prefixes, optionals, overflow, effects.
- [[memory-model]] — value semantics, reference counting, parameter conventions.
- [[colorless-concurrency]] — green threads and where colorless runtimes leak.

**The tools**
- [[semantic-compiler]] — diagnostics, the repair loop, digest, LSP and MCP.
- [[constrained-decoding]] — grammars, the APIs that accept them, type constraints.

**The plan**
- [[transpilation-strategy]] — Python and C targets, the Python→lotml corpus.
- [[evaluation-harness]] — metrics, sample size, gates.
- [[lotml-risks]] — the revised risk table.
- [[requirements-and-roadmap]] — revised and new requirements, phases and gates.

## Decisions

The decisions that are expensive to reverse were accepted on 2026-10-05; 0004 and 0005 state
which harness result would lead to replacing them:

- adr:0001-transpile-to-python-first
- adr:0002-errors-as-values
- adr:0003-value-semantics-with-reference-counting
- adr:0004-python-syntax-where-semantics-match
- adr:0005-significant-indentation
- adr:0006-compiler-written-in-rust

## How this study was done

- **Literature:** three independent surveys (grammars and corpus; compiler and decoding; memory
  and runtime), each claim checked against its primary source. Claims seen only in a search
  result were left out or are flagged.
- **Measurements:** `research/tokens/` (counter, paired corpus, results) and `research/pilot/`
  (specs, tasks, grammar, checkers, generated programs, results), reproducible with `uv run`.
- **Limits:** the Claude tokenizer was not measured; the pilot used only Claude models and did not
  execute programs; the paired corpus has a single author.
