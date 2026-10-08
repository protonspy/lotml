# LLM-oriented language

lotml is a programming language designed to be written by LLMs and reviewed by people:
Python-like syntax, static types, errors as values, value semantics and a compiler built as an
agent's tool. This is the study's entry page — what was verified, what changed relative to the
original proposal of 2026-10-05, and where each part lives. A second pass on the same day read
every source in full, checked each quoted number against the downloaded paper
([[source-verification]]) and settled six open questions by experiment.

## Conclusions

1. **The thesis holds, but the bottleneck is not syntax.** Compile failures are mostly names,
   methods and types, and most failures are not compile failures at all but logic errors that
   tests catch. What decides is the [[semantic-compiler]], a [[type-system]] checkable while the
   code is being written, and `test` blocks in the loop.
2. **The token saving from syntax is about 10%**, not the original target's 20%; the language
   effect on agentic cost is larger and comes from failures, not verbosity ([[token-cost]]).
3. **The corpus risk moved from syntax to fidelity and semantics.** With the spec in the prompt,
   three Claude models wrote no Python syntax in 60 programs; the errors were semantic habits, and
   executing the programs found code whose result a Python reader would predict wrongly
   ([[python-leakage-pilot]]). The literature agrees: a short, example-based syntax reference fixes
   most syntax, and familiar syntax with a new meaning is the costliest kind ([[training-prior]]).
   That holds for frontier models only: at the phase 1 gate, `lotml check` refused 91 (Qwen 2.5
   Coder 7B) and 143 (Llama 3.1 8B) of 200 first answers, which also wrote syntax lotml does not
   have (adr:0011-proceed-to-phase-2-past-the-failed-phase-1-gate).
4. **Where the semantics are Python's, the syntax should be Python's — and where they differ, the
   difference must be visible.** Variant B costs 0.8 percentage points more tokens and removes two
   traps that execution showed in model-written variant A code ([[lotml-syntax]]).
5. **Significant indentation stays.** The editing test at scale met the condition: no model edited
   worse in the indented form, on 192 paired tasks with four models
   (adr:0010-variant-b-and-indented-blocks-settled-by-the-phase-0-gate); a misplaced brace is silent
   more often than a misplaced line, the edit interface matters more than
   the block style, and a depth-bounded grammar constrains it at no extra decode cost
   ([[editing-robustness]], [[constrained-decoding]]).
6. **Value semantics with reference counting is the right choice**, with explicit parameter
   conventions, values marked shared when handed to a task, and reuse tuned rather than assumed —
   the headline benchmarks are best cases ([[memory-model]]).
7. **The compiler checks while the model writes.** Checking streamed prefixes with the real
   compiler works on closed models; diagnostics carry admissible alternatives; reports are small and
   root-cause first; edits can address symbols ([[semantic-compiler]]).

## The original study, claim by claim

| original study's claim | verdict | where |
| --- | --- | --- |
| "less verbose" means fewer tokens, not fewer characters | confirmed | [[token-cost]] |
| rare symbols and abbreviations cost more tokens | **corrected:** APL glyphs cost 2–4 tokens; `cnt`, `idx`, `msg` cost 1, like the full word | [[token-cost]] |
| the gain comes from removing boilerplate, not shortening names | confirmed: a record saves 7 tokens, `fn` versus `def` saves 0 | [[token-cost]] |
| `str?` costs 1 token instead of 3–4 | **refuted:** it saves 1 against both `Optional[str]` and `str \| None` | [[token-cost]] |
| the proposed version has less than half the lines | true in lines (−62%); in tokens, −12% to −16% | [[token-cost]] |
| programs ≥ 20% smaller in tokens than Python | **refuted:** 9–11% against typed Python on eight tokenizers; compressed syntaxes reach 8–11% in generated code | [[token-cost]] |
| mandatory signatures and local inference help most | confirmed, with an adjustment: inference checkable on prefixes, signatures before bodies, a placeholder for unfinished holes | [[type-system]] |
| the compiler is the most important half | **qualified:** documentation and compiler tools work only together; the compiler's new role is checking while the model writes | [[semantic-compiler]] |
| fixes with numeric confidence | **corrected:** applicability levels, as in rustc, plus admissible alternatives | [[semantic-compiler]] |
| the digest brings more gain than trimming the syntax | partly: it improves localization and cost; generation still needs the bodies, and search the rest | [[semantic-compiler]] |
| a published grammar prevents invalid code | **corrected:** it only covers syntax, a syntax-only mask can lower accuracy, and only OpenAI and open models accept grammars; prefix checking serves every model | [[constrained-decoding]] |
| risk number one is the lack of a corpus | **corrected:** for frontier models with the spec, the risk is implementation fidelity and semantics, not syntax | [[training-prior]] |
| no language has been designed for LLMs | **outdated:** BAML, MoonBit, Pel, NanoLang, Zero, Quasar, Anka — only Anka with evidence the design helps, and weak | [[languages-for-agents]] |
| `use` avoids implicit relative imports | **corrected:** Python 3 no longer has implicit relative imports | [[lotml-syntax]] |
| LLMs get Rust lifetimes wrong a lot | **corrected:** ownership is 16.7% of self-contained Rust compile errors; type mismatches lead | [[memory-model]] |
| mutable values with ARC and elision | confirmed, with non-atomic counting, sharing marked at spawn and `inout` conventions | [[memory-model]] |
| configurable overflow in release | **corrected:** one semantics in every build and on every target; trapping costs about 2.4× on CPython | [[type-system]], [[transpilation-strategy]] |
| colorless concurrency eliminates the forgotten `await` | plausible, not measured | [[colorless-concurrency]] |
| transpile to Python, then C, then native | confirmed, with a minimal transpiler already in phase 0 — and tracebacks verified to point at the lotml source | [[transpilation-strategy]] |
| indentation-based blocks | kept, and settled by the phase 0 gate | [[editing-robustness]] |

## Pages

**What was measured**
- [[token-cost]] — eight tokenizers, a 12-task paired corpus, isolated constructs.
- [[python-leakage-pilot]] — three models, two variants, 60 programs, parsed and executed.
- [[editing-robustness]] — slips in both block styles, the editing pilot, edit formats.
- [[source-verification]] — 58 papers downloaded, 791 quotes checked, the corrections.

**The context**
- [[training-prior]] — why new languages start near zero, and how to get around it.
- [[languages-for-agents]] — what already exists and what little of it was measured.

**The design**
- [[lotml-syntax]] — the rule, variants A and B, where semantics differ, indentation.
- [[type-system]] — types checkable on prefixes, optionals, overflow, effects.
- [[memory-model]] — value semantics, reference counting, parameter conventions.
- [[colorless-concurrency]] — green threads and where colorless runtimes leak.

**The tools**
- [[semantic-compiler]] — diagnostics, the repair loop, prefix checks, digest, LSP and MCP.
- [[constrained-decoding]] — grammars, the APIs that accept them, indentation, type constraints.

**The plan**
- [[transpilation-strategy]] — Python and C targets, the Python→lotml corpus.
- [[evaluation-harness]] — metrics, sample size, gates.
- [[lotml-risks]] — the revised risk table.
- [[requirements-and-roadmap]] — revised and new requirements, phases and gates.

## Decisions

The decisions that are expensive to reverse were accepted on 2026-10-05; 0004 and 0009 state
which harness result would lead to replacing them:

- adr:0001-transpile-to-python-first
- adr:0002-errors-as-values
- adr:0004-python-syntax-where-semantics-match
- adr:0006-compiler-written-in-rust
- adr:0007-integer-division-returns-f64
- adr:0008-value-semantics-with-reuse-before-borrowing
- adr:0009-significant-indentation-with-symbol-addressed-edits
- adr:0010-variant-b-and-indented-blocks-settled-by-the-phase-0-gate

The second pass supports all six original decisions; two records cited evidence it corrected.
adr:0003-value-semantics-with-reference-counting credited Lean's speed over OCaml to borrow
inference and presented Perceus's best case as typical, and adr:0005-significant-indentation read
SWE-agent's guard as an indentation guard ([[source-verification]]). Both were superseded with the
corrected context: 0008 keeps value semantics and puts reuse before borrow inference, and 0009 keeps
indentation and adds edits addressed to symbols.

## How this study was done

- **Literature:** a first pass with three independent surveys; a second pass that downloaded 58
  papers, converted them with docling, read each in full and checked every quoted number
  mechanically (`research/literature/`).
- **Measurements:** `research/tokens/`, `research/pilot/` and `research/experiments/` — tracebacks,
  overflow cost, sample size, a research transpiler that executes the pilot, indentation slips,
  grammars under llguidance, and the editing pilot — each reproducible with `uv run`.
- **Limits:** the Claude tokenizer was not measured; the pilots used only Claude models and the
  models' own tests or small hidden ones; the paired corpus has a single author. The phase 0 and
  phase 1 gates later added two 7–8B open models, and phase 1 found them refused far more often
  than the Claude models (adr:0011-proceed-to-phase-2-past-the-failed-phase-1-gate).
