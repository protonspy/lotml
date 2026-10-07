# Changelog

What changed in the wiki, and when. Newest first, one line per change, naming the
pages it touched — this is the log that says whether a page was revisited after the
thing it describes moved.

A page named here that no longer exists is reported, so a rename is recorded as a
change rather than left pointing at the old slug.

<!-- Newest first, as `- 2026-01-14 — added [[page-slug]], folded [[other-page]]
into it`. -->

- 2026-10-07 — sixteen compilers and interpreters of Python and its derivatives read against lotml's compiler: added [[prior-art-compilers]], [[target-parity]] and [[compiler-performance]]; [[semantic-compiler]], [[transpilation-strategy]] and [[evaluation-harness]] link them
- 2026-10-07 — the phase 4 targets: [[transpilation-strategy]] tells the C target of phase 3 in the past, adds the one IR both targets read, the LLVM target, `--shared` and their parity and benchmarks; [[memory-model]] and [[evaluation-harness]] add the LLVM target's measurements beside the C target's
- 2026-10-07 — `lotml build` makes a native executable by default (adr:0022): [[transpilation-strategy]] names `--target python` where `build` writes the module Python imports
- 2026-10-07 — training a small coder: added [[small-coder-training]], [[grpo]], [[verifiable-rewards]], [[rl-environment]], [[reward-hacking]] and [[repair-training]] from 55 papers checked in [[source-verification]]; corrected Agnostics' model sizes in [[training-prior]] and linked the pages from [[compiler-embedded-model]]
- 2026-10-07 — `.lot` beside `.lotml` (adr:0018): [[semantic-compiler]] names both extensions and the guide `init` writes as `lotml.guide.lot`
- 2026-10-06 — the C target built and measured: [[transpilation-strategy]] describes it, [[evaluation-harness]] reports the phase 3 gate, [[memory-model]] the benchmarks against C
- 2026-10-06 — `lotml init` and the agent harness: [[semantic-compiler]] describes the agent guide, [[evaluation-harness]] the agent benchmark and its arms (adr:0014)
- 2026-10-06 — type masks by the line measured on three open models: [[constrained-decoding]] reports that they helped only the weakest
- 2026-10-06 — terse against detailed diagnostics and the forgotten-`await` hypothesis measured: [[semantic-compiler]] and [[colorless-concurrency]] report them
- 2026-10-06 — the tree-sitter grammar for editors generated from the parser's source: [[constrained-decoding]] describes it, [[requirements-and-roadmap]] marks it built
- 2026-10-06 — C libraries callable from lotml: [[transpilation-strategy]] describes the interfaces named `c.<library>` (adr:0013)
- 2026-10-06 — colorless concurrency built on the Python target: [[colorless-concurrency]] says what `parallel` does and the capture bug it found
- 2026-10-06 — the Python→lotml corpus built, 509 programs validated by their tests: [[training-prior]] reports it, [[transpilation-strategy]] describes the pipeline
- 2026-10-06 — Python and lotml calling each other built: [[transpilation-strategy]] describes the checked boundary and the interfaces `lotml bind` writes (adr:0012)
- 2026-10-06 — symbol-addressed edits and atomic rename built: [[semantic-compiler]] describes them, [[editing-robustness]] marks its design as built
- 2026-10-06 — the language server and the MCP server built: [[semantic-compiler]] says what phase 2 adds
- 2026-10-06 — prior work on a small model inside the compiler surveyed from 25 checked papers: added [[compiler-embedded-model]], linked from [[semantic-compiler]]; [[source-verification]] counts the new sources and quotes
- 2026-10-06 — phase 2 proceeds past the failed phase 1 gate by adr:0011: [[evaluation-harness]] cites it
- 2026-10-06 — the phase 1 gate failed on pass@1, so phase 2 has not started: [[evaluation-harness]] records the gate and why, [[lotml-risks]] measures the training-corpus risk
- 2026-10-06 — the compiler built: [[semantic-compiler]] lists what phase 1 shipped, [[transpilation-strategy]] the Python backend
- 2026-10-05 — the phase 0 gate passed and adr:0010 freezes variant B and indented blocks: [[evaluation-harness]] records the gate, [[lotml-syntax]], [[llm-oriented-language]], [[requirements-and-roadmap]] and [[lotml-risks]] drop the condition
- 2026-10-05 — the indented form against braces edited at scale, 192 tasks with four models: [[editing-robustness]] reports it, [[evaluation-harness]] lists it as built
- 2026-10-05 — variant B against variant A repeated at scale with four models: [[lotml-syntax]] reports it
- 2026-10-05 — the Claude tokenizer measured on the paired corpus and on model-written code: [[token-cost]] closes the gap it listed
- 2026-10-05 — the grammar published in three dialects: [[constrained-decoding]] describes how they are generated and tested
- 2026-10-05 — the harness executor built: [[evaluation-harness]] lists it under what exists
- 2026-10-05 — the harness task set built: [[evaluation-harness]] lists it under what exists
- 2026-10-05 — integer division decided and ADRs 0003 and 0005 superseded with the corrected evidence: [[type-system]], [[lotml-syntax]], [[requirements-and-roadmap]], [[memory-model]], [[editing-robustness]], [[evaluation-harness]] and [[llm-oriented-language]] cite adr:0007, adr:0008 and adr:0009
- 2026-10-05 — evidence deep dive: added [[source-verification]] and [[editing-robustness]]; corrected numbers and conditions against the downloaded papers and added the experiments' results in [[llm-oriented-language]], [[semantic-compiler]], [[constrained-decoding]], [[training-prior]], [[memory-model]], [[token-cost]], [[type-system]], [[lotml-syntax]], [[python-leakage-pilot]], [[transpilation-strategy]], [[evaluation-harness]], [[languages-for-agents]], [[colorless-concurrency]], [[lotml-risks]] and [[requirements-and-roadmap]]
- 2026-10-05 — wiki translated into English, with English slugs: `linguagem-orientada-a-llms` → [[llm-oriented-language]], `custo-em-tokens` → [[token-cost]], `piloto-de-vazamento-de-python` → [[python-leakage-pilot]], `prior-de-treino` → [[training-prior]], `linguagens-para-agentes` → [[languages-for-agents]], `sintaxe-do-lotml` → [[lotml-syntax]], `sistema-de-tipos` → [[type-system]], `modelo-de-memoria` → [[memory-model]], `concorrencia-sem-cor` → [[colorless-concurrency]], `compilador-semantico` → [[semantic-compiler]], `decodificacao-restrita` → [[constrained-decoding]], `estrategia-de-transpilacao` → [[transpilation-strategy]], `harness-de-avaliacao` → [[evaluation-harness]], `riscos-do-lotml` → [[lotml-risks]], `requisitos-e-roadmap` → [[requirements-and-roadmap]]
- 2026-10-05 — lotml's file extension set to `.lotml`: [[transpilation-strategy]], [[evaluation-harness]] and [[python-leakage-pilot]]
- 2026-10-05 — the language is named lotml: `sintaxe-da-linguagem-x` renamed to [[lotml-syntax]] and `riscos-da-linguagem-x` to [[lotml-risks]]; the other pages replace the provisional name X with lotml
- 2026-10-05 — ADRs 0001 to 0006 accepted; [[llm-oriented-language]], [[lotml-syntax]] and [[requirements-and-roadmap]] cite them as decisions
- 2026-10-05 — LLM-oriented language study distilled from `docs/raw/`: added [[llm-oriented-language]], [[token-cost]], [[python-leakage-pilot]], [[training-prior]], [[languages-for-agents]], [[lotml-syntax]], [[type-system]], [[memory-model]], [[colorless-concurrency]], [[semantic-compiler]], [[constrained-decoding]], [[transpilation-strategy]], [[evaluation-harness]], [[lotml-risks]] and [[requirements-and-roadmap]]
