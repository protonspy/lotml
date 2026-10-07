# Languages designed for LLMs — methodology and measured results

Prior-art study for LotML, 2026-10-07. It extends, and does not repeat, the wiki pages
`languages-for-agents`, `llm-oriented-language`, `training-prior`, `token-cost` and
`constrained-decoding`; papers those pages already cite (typed holes 2409.00921, Nogueira et al.
2608.00661, RustRepoTrans, Generative Compilation, JSONSchemaBench, SimPy, Token Sugar, Tokenmaxxing,
PyLang, CangjieBench, SPEAC and the rest) are referred to, not summarised again. Harnesses and
training are in the sibling study `evaluation-and-adaptation.md`.

**How numbers were checked.** Each number carries a mark:
- **[V]** — read on the primary text for this file (arXiv PDF text, official README via `gh api`,
  official page) on 2026-10-07.
- **[A]** — read on the primary source by one of the research passes behind this file, not re-read
  here.
- **[U]** — unverified (secondary source, interactive chart, or search snippet only).

Repository metadata (license, stars, last push, creation date) comes from `gh api` on 2026-10-07 [V].
Model names such as "Claude Fable 5", "GPT-5.6 Sol" or "GPT 6 Astra" are copied as the sources print
them. No code was cloned or run.

## Summary of findings

1. The family grew fast: at least nine more languages built for models to write appeared in 2025–26 (Vera, Aver, AILANG, Almide, Mog, Jacquard, NERD, Codong, Sui/Isu), plus four whose compiler calls a model (CodeSpeak, Djinnlang, Plang, Marsha).
2. The first cross-language measurement of zero-training-data languages exists: VeraBench. Vera averaged 98.7%, Python 96.7% and TypeScript 99.7% across nine frontier models (60 problems, single run, saturated). Vera beat Aver by 1–10 points on five models [V].
3. The training prior still dominates outside small benchmarks. On LeetCode, DeepSeek-V3 scored 79.81% in Python against 24.31% in Erlang. Python leads Multi-LCB with a mean pass@1 of 0.482. Models implement in Python 35.3% of the time but recommend it only 10.7% of the time [V].
4. Static types pay off through the feedback loop, not through the annotations themselves:
   - Moving to a weaker model raised the fix-commit ratio 21.6 points in TypeScript but 38.4 points in Python [V].
   - Python's VeraBench failures were runtime wrong answers 13 times, against 3 compile rejections [V].
   - On the other side, `mypy --strict` cost 1.6–1.7× agent time on a small task [V].
5. Indentation: the only measured harm is to Python patch generation (full-function patches misaligned, every model except Gemini) [V]. No controlled study of indentation against braces exists apart from LotML's own gate.
6. Errors as values: no study compares result types with exceptions. Exception handling accounts for 11.6–16.75% of SonarQube bug findings in LLM-written Java [V]. Without help, models catch almost nothing (Seeker baseline coverage 13%) [A].
7. Verification targets rank Dafny > Verus > Lean, consistently: 82/44/27% on vericoding, 40.3/24.7/7.8% on the aligned AlgoVeri [V]. Familiar host syntax (Nagini for Python, Verus for Rust) does not carry the proof layer. Writing Dafny and compiling it to Python scored 77%, against 86% for writing Python directly [V].
8. Intermediate-language designs keep winning when the model writes Python-shaped code and a compiler lowers it. Quasar's checked subset with static repair beat unrestricted Python on AgentDojo execution: 89.2 against 76.3 [V].
9. Three wiki entries need correcting:
   - Anka's +40 points comes from a category defined as 3–5 operations.
   - MoonBit published absolute numbers: 43.75% → 56.25% compile rate.
   - Zero is now "zerolang", quiet since June [V].
10. No finding conflicts with an accepted ADR. Several sharpen requirements: edits addressed to symbols as the indentation mitigation, a typed/untyped ablation, and contracts, if ever added, discharged by an SMT solver.

## What changed since the wiki page

| entry | wiki baseline | what changed or was missed | marks |
| --- | --- | --- | --- |
| BAML | pre-1.0, nightlies | **Two release lines**: the new "BAML language toolchain" (blog: "BAML v1"; `baml-language-0.20.1`, 2026-09-20, nightlies to 2026-10-07) and the classic client generator `0.226.2` (2026-09-01). Still "pre-1.0". New agent-facing tools: `baml agent install`, `baml describe` ("AST-aware code discovery"). No evaluation of the language design. Vendor-run SAP numbers: below. Apache-2.0, 9,382★, pushed 2026-10-07. | [A], metadata [V] |
| MoonBit | sampler gain "without absolute numbers" | **Numbers exist.** LLM4Code 2024 position paper: raw decoder 43.75% vs semantics-based sampler 56.25% compile rate, throughput 12.10 vs 11.75 tokens/s (~3% penalty). CodeLlama-34B, 5-bit GGUF on llama.cpp, "33 relatively simple coding tasks", one model, no tests or pass@k. Both rates are k/32, not k/33; the paper does not explain the denominator. **1.0 slipped:** the 2025 roadmap targeted H1 2026; the blog through 2026-09-01 has no 1.0. **New:** MoonBit 0.9 (2026-04-08) adds first-class formal verification (`moon prove`); SeekMoon "ADE" (2026-09-01); SWE-AGI benchmark (below). | [V] |
| Pel | no evaluation | Unchanged. v2 (2025-06-09) only fixed an email and fonts. No public implementation found. | [A] |
| Quasar | 71.4 vs 71.8; −26% approvals; −18–27% time | Those numbers come from **v2** (arXiv stamp 25 Aug 2026, "Published as a conference paper at COLM 2026"). v2 adds AgentDojo (93 tasks) and BrowseComp-Plus (80 tasks) to GQA (1,000 tasks); programs written by GPT-5. Execution / accuracy, GQA and AgentDojo: Python 99.6/76.3, 71.8/64.5; subset 97.9/82.8, 71.4/63.4; **multi-turn repair from transpiler errors only 99.5/89.2, 72.3/67.7**. The −26% approvals holds on both GQA and AgentDojo; −18% time is GQA, −27% AgentDojo. BrowseComp-Plus: −97% approvals, 93% speedup. SFT of GPT-4.1-nano on 900 examples: execution 92 → 99. Artifact repo has no license. | [V] |
| Anka | +40 points "on pipelines of five or more steps" | Table 2 defines `multi_step` as "3–5 sequential operations"; only Figure 1 labels the +40 "complex (5+ operations)". Multi-step 100 vs 60 (Claude 3.5 Haiku); GPT-4o-mini +26.7 on that category only. Inconsistencies: 10 tasks per category cannot give 96.7 or 86.7 as task fractions; the adversarial category is missing from Table 3; no statistical tests. Repo: MIT, 18★, one commit. | [V] |
| NanoLang | mandatory tests, JSON spec, no results | v5.0.0 (2026-09-16). Now also a "secure runtime" (verified bytecode VM, capabilities). Claims Coq proofs of type soundness. `LLM_CORE_SUBSET.md` ("~50 primitives"). **Mandatory tests softened:** "Missing-shadow enforcement is not universal." Still no evaluation. One anecdote (Simon Willison, 2026-01-19): Opus 4.5 with `MEMORY.md` wrote code that did not compile until given examples. Apache-2.0, 628★. | [A], metadata [V] |
| Zero | JSON diagnostics; turned "graph-native" | Renamed **zerolang** (`vercel-labs/zerolang`, zerolang.ai). Last release v0.3.4 (2026-06-13); 3 commits since 2026-06-20, so effectively dormant. Graph-first since v0.3.0: `zero.graph` is the input, `.0` files are projections, `zero patch` edits are guarded by graph hashes. Diagnostics keep stable codes (`NAM003`) with rule/expected/actual/fix fields, plus `zero fix --plan --json`. New nuance: "Agents should not default to JSON for every command." An `evals/` harness (Claude Code, Opus 4.7 / Sonnet 4.6, prompt "intentionally avoids Zero syntax examples") has **published no results**. Apache-2.0, 5,382★. | [A], metadata [V] |

The wiki's verdict that "almost none has published evidence" now needs an update. There are three measured
comparisons (VeraBench, intent-trace, Almide's dojo), one large benchmark in a new language (SWE-AGI on
MoonBit), and Quasar v2. None has LotML's paired design with significance tests.

## New languages — overview

Built for models to write (A), compiler calls a model (B), DSL or intermediate language for generation (C).

| language | kind | decisions made for LLMs | evaluation | headline result | repo · license · ★ · last push | maturity |
| --- | --- | --- | --- | --- | --- | --- |
| Vera | A | no variable names (typed slots `@Int.0`); mandatory `requires`/`ensures`/`effects` checked by Z3; algebraic effects; coded diagnostics with a fix example; LSP; SKILL.md | VeraBench: 60 problems, 9 models, against Python, TS, Aver, AILANG | 98.7% vs Py 96.7% vs TS 99.7% | aallan/vera · MIT · 417 · 2026-10-06 | experimental, very active |
| Aver | A | effects in signatures; `decision` blocks; `verify` blocks; record/replay; Lean proof export; `aver agent-connect` installs skills | intent-trace (diff legibility, ~19,000 judgments); VeraBench | Aver = Aver-in-Python within noise; 92.4% on VeraBench (5 models) | jasisz/aver · MIT · 61 · 2026-10-07 | experimental |
| AILANG | A | pure, effect-typed, "deterministic execution substrate"; written by AI agents; MCP tools and Claude Code / Codex plugins | own dashboard (loaded dynamically, not extracted); VeraBench | 96.8% on VeraBench (5 models) | sunholo-data/ailang · Apache-2.0 · 34 · 2026-10-07 | experimental |
| Almide | A | optimises "modification survival rate"; one canonical form; diagnostics carry fix templates; surface frozen in STABILITY.md and `llms.txt`; Perceus-style ownership inference; Rust and Wasm targets | almide-dojo, 38 tasks, temperature 0; MiniGit (mame), Sonnet 5 × 20 | Llama 3.3 70B 65%, Llama 3.1 8B 44%; MiniGit 100% | almide/almide · MIT/Apache-2.0 · 34 · 2026-10-07 | pre-1.0, very active |
| Mog | A | whole spec fits in context (~3,200 tokens); familiar Rust/Go/TS syntax; host-granted capabilities; in-process QBE backend | none | — | voltropy/mog · MIT · 144 · 2026-03-09 | experimental, quiet |
| Jacquard | A | AI-written, human-reviewed; runtime-enforced effects in every signature; content-addressed definitions; swappable "worlds" | none | — | jbwinters/jacquard-lang · Apache-2.0 · 120 · 2026-10-07 | research prototype |
| NERD | A | English keywords instead of symbols; LLVM IR; humans audit, not author | one token count | 32 tokens vs JS 70, tokenizer unstated | Nerd-Lang/nerd-lang-core · Apache-2.0 · 171 · 2026-01-24 | "not ready", dormant |
| Codong | A | one way per task; JSON errors with a `fix` field; 8 built-in modules | one CRUD "arena" run | 955 tokens vs Python 1,867 | brettinhere/Codong · MIT · 73 · 2026-04-12 | toy/experimental |
| Sui → Isu | A | numbered variables; independent lines; Isu = pseudocode ⇄ JSON AST | none | own table: Sui *larger* than Python (79 vs 30 tokens) | TakatoHonda/sui-lang · MIT · 370 · 2025-12-16 | pivoting |
| CodeSpeak | B | Markdown specs are the source; an LLM generates and maintains code | none public | "5–10× smaller" claim [U] | no public compiler | waitlist |
| Djinnlang | B | spec-only programs lowered to Dafny; LLM fills code and proofs; spec must be provably unambiguous | 3 programs, 2 agents; self-hosted translator | 256/256 obligations discharged | paper only (arXiv 2609.23954) | research |
| Plang | B | natural-language steps compiled by an LLM to JSON | none | — | PLangHQ/plang · LGPL-2.1 · 64 · 2026-10-05 | v0.1 |
| Marsha | B | descriptions plus examples; the LLM writes tests, then Python | none ("aim for 80%+") | — | alantech/marsha · MIT · 467 · 2026-10-07 | alpha (2023 start) |
| Flint | C | JSON visualization IL with semantic types; compiled to Vega-Lite / ECharts / Chart.js; MCP server | 315 questions, 3 models, VLM judge, win/tie/loss | beats direct Vega-Lite, p ≤ 0.05 | microsoft/flint-chart · MIT · 4,347 · 2026-10-07 | used |
| AIDL | C | CAD DSL; geometric constraints handed to a solver, not the LLM | GPT-4o, 36 prompts × 10, vs OpenSCAD, ablations | CLIP 28.90 vs 27.32; success 64% vs 79% | paper (arXiv 2502.09819) | research |

Excluded: languages humans write to call LLMs (llm-lang, Corvid, Dana, Convo-Lang); closed or unsubstantiated
claims (Revia; U language, "96% correct first try" [U]; AISP [U]); toys under 20★ with no paper.

## Per-language detail

### Vera — the closest analogue to LotML
- **Design** [V]:
  - **No names.** `@Int.0` is the most recent `Int` binding. The README justifies this with literature on
    naming errors (arXiv 2307.12488).
  - **Contracts.** Mandatory `requires`/`ensures`/`effects` on every function. Z3 proves them, and what
    it cannot decide becomes a runtime check.
  - **Effects.** `<Inference>` and `<Http>` effect rows.
  - **Diagnostics** are "instructions for the model": what went wrong, why, a fix with code, and a spec
    reference, under stable codes (`E001`…).
  - **Target.** Compiles to WebAssembly.
  - Compiler written in Python [A]. v0.2.0 (2026-09-26) [A].
- **Method** (VeraBench README, against Vera v0.1.8) [V]:
  - 60 problems in five tiers. Tier 5 (effects) is excluded from cross-language headline rates.
  - Nine models from three providers. Single run, no pass@k. "One problem is worth 1.7 percentage points."
  - Metrics: pass@1, check@1, verify@1, fix@1.
  - Baselines: Python, TypeScript, and the zero-training-data languages Aver and AILANG. "Vera NL"
    variant: the model writes the contracts itself.
- **Results** [V]:
  - **Per model** (Vera / Vera NL / Python / TS):

    | model | Vera | Vera NL | Python | TS |
    | --- | --- | --- | --- | --- |
    | Claude Fable 5 | 100 | 97 | 97 | 97 |
    | GPT-5.6 Sol (pro) | 100 | 90 | 95 | 100 |
    | Claude Opus 5 | 100 | 95 | 95 | 100 |
    | Claude Opus 4.8 | 93 | 93 | 98 | 100 |
    | GPT-5.6 Sol | 98 | 92 | 95 | 100 |
    | Kimi K3 | 100 | 92 | 100 | 100 |
    | Claude Sonnet 5 | 97 | 87 | 98 | 100 |
    | GPT-5.6 Terra | 100 | 92 | 95 | 100 |
    | Kimi K2.6 | 100 | 93 | 97 | 100 |

  - Vera beats Python for six of nine models by 3–5 points, draws one, loses two. Against TS it wins one,
    draws five, loses three.
  - Python's failures were runtime wrong answers 13 times against 3 compile rejections; TypeScript had
    one of each.
  - Zero-training-data comparison, five-model average: Vera 98.2, AILANG 96.8, Aver 92.4, Python 97.0.
  - Across Claude generations, Vera gained 11 points and Python 1. Only the last step was controlled;
    the earlier one also changed compiler, standard library and skill file.
- **Caveats, by the author** [V]:
  - The benchmark is saturated: TypeScript is at 100% for 8 of 9 models.
  - A grader issue (#121) may inflate the Python gap.
  - Vera against Python confounds design with training data.
- **Our reading:** for frontier models with a skill file on small problems, novelty is no longer the
  bottleneck. LotML's phase 1 gate found the same: Sonnet at 97.0% against 97.5%. Nothing here speaks to
  7–8B models.

### Aver and intent-trace — a control for "format vs language"
- **Design** [V]:
  - A Rust toolchain with VM, Rust, Wasm and Lean backends.
  - Effects in signatures; `decision` blocks (reason / chosen / rejected); `verify` blocks.
  - Deterministic record/replay.
  - `aver agent-connect` writes `.claude/skills/aver/SKILL.md` and points `AGENTS.md` at it, so the
    language guide ships inside the binary.
- **intent-trace** [V]: ~19,000 judgments. Six reader LLMs each guess a change's intent from the raw diff,
  with no language guide; six cross-vendor judges score the guesses. The design is 18 prompts × 4
  programs × 3 variants: Aver, **Aver transliterated to Python**, and idiomatic OOP Python.
  - Aver and Aver-in-Python tie within the 0.11 noise floor for four of six readers on full diffs.
  - Idiomatic Python sits 0.13–0.59 below on strong readers.
  - With prose stripped, Aver-in-Python wins by 0.03–0.66.
- **Lesson:** the structure (declared intent, decisions, specs) carries the legibility, not the novel
  syntax. This is the ablation the languages-for-agents family usually skips. Small scale; no license on
  the repo.

### AILANG
- **Design** [V]: purely functional and effect-typed, "a deterministic execution substrate for
  AI-generated code". Agents wrote it autonomously through its own coordinator.
  - Ships MCP tools (`ailang_prompt`, `ailang_check`, `ailang_run`, `ailang_builtins`, `ailang_eval`)
    and plugins for Claude Code and Codex.
  - Agents are told to run `ailang prompt` before writing `.ail` code; the CLI is the source of the
    current syntax.
- **Results:** its benchmark dashboard loads dynamically; no numbers extracted [U]. External: 96.8% on
  VeraBench (five-model average) [V].

### Almide
- **Design** [V]:
  - Built for one metric, "modification survival rate": does code still compile and pass after a series
    of AI edits?
  - Principles: predictable (one canonical form), local, repairable, compact.
  - Perceus-style ownership inference, with the checker proved in Rocq/Coq.
  - Native code through Rust, plus direct Wasm, with byte-identical output.
  - A CI script refuses README numbers without a date or a run.
- **Results** [V]:
  - almide-dojo, 38 tasks, almide 0.62.0, seed and temperature 0, 2026-09-22:
    - Llama 3.3 70B 65% (25/38), with 39% (15/38) on the second column, labelled 1-shot [A].
    - Llama 3.1 8B 44% (17/38) and 34% (13/38).
    - No other language was run on the dojo.
  - MiniGit (mame's task), Sonnet 5 × 20 trials, almide 0.29.0: 100% pass, the fewest lines of five
    languages (233), and agent wall-clock 573 s against 297 s for the fastest (Rust and TS).
- **Relevance:** the nearest architectural cousin. Rust-hosted compiler, reference counting with reuse,
  native and Wasm targets, and a published claim discipline LotML's harness shares.

### Mog, Jacquard, NERD, Codong, Sui/Isu, B-IR
- **Mog** [V/A]: "statically-typed Lua" embedded in a host. The host grants capabilities
  (`requires http, model`), so an agent's permissions bound the code it writes. Its rule: "the entire
  language should fit in an LLM's context window". No evaluation.
  - HN (163 points) objected: "given there is no training dataset, any other language would work better
    with AI" [A].
- **Jacquard** [V/A]: reviewer-first ("what can this touch?"), with runtime-enforced effect sets. Built in
  OCaml, emitting C. Research prototype, no evaluation.
  - HN: multi-shot effects are "incredibly difficult to reason about for humans" [A].
- **NERD** [A]: 32 tokens against JavaScript's 70 for four arithmetic functions, tokenizer unstated. Not a
  measurement of correctness.
- **Codong** [A]: one Claude Sonnet 4 run of a CRUD API. Python cost 1,867 tokens; the "70%+ savings"
  headline is an estimate table.
- **Sui** [A]: its own token table shows it *larger* than Python. It is pivoting to **Isu**, a structured
  pseudocode parsed to a JSON AST for step-level repair loops [V].
- **B-IR** [A]: a Gemini-designed, token-minimal language that its author judged "underwhelming". He
  concluded that low ambiguity, avoiding loose typing, little whitespace significance and tests next to
  the code matter more than token density.
- **Weft** [A]: a toy (0★) that used 3.20–3.70× *more* tokens than Python on three tokenizers.
- **Pattern:** token-density claims in this group are unmeasured or refuted, consistent with the wiki's
  `token-cost` page.

### Languages whose compiler calls a model
- **Djinnlang** (Henniger, Chong, Amin; arXiv 2609.23954, 2026-09-21) [V]:
  - **Design.** Programs are specifications. A translator lowers them to Dafny stubs and proof
    obligations; an LLM agent writes the implementation and proofs; Dafny verifies. An "unambiguity
    constraint" proves the specification deterministic, so regenerated code behaves identically.
  - **Benchmark.** Three programs (Sudoku, LZ77, an Imp VM). Both implementers discharged all 256
    obligations.
  - **Cost.** Codex took 33 minutes and about $47; Claude Code took 21 hours and $1,026.
  - **Self-hosting.** The translator spec has 2,106 obligations; GPT 6 Astra spent 1.4 billion tokens
    (about $1,950 at API prices) on it.
- **CodeSpeak** (Andrey Breslav):
  - Markdown specs are the source of truth. No public compiler; the "5–10× smaller codebase" claim is
    [U].
  - HN (318 points) objected to non-determinism and lossy specs [A].
- **Plang, Marsha**: build-time LLM compilation, no measurements [A].

### DSLs designed for model generation
- **Flint** (Microsoft Research; arXiv 2607.20775) [V]. 315 questions over 63 tables, compared with agents
  writing Vega-Lite directly:

  | model | Flint wins | ties | Vega-Lite wins | p |
  | --- | --- | --- | --- | --- |
  | GPT-5.1 | 129 | 65 | 118 | 0.05 |
  | GPT-5-mini | 140 | 65 | 106 | 0.001 |
  | GPT-4.1 | 135 | 70 | 106 | 0.004 |

  Its specs are "on average 85% shorter". HN: "That's no language, that's a JSON schema" [A].
- **AIDL** (Jones et al., arXiv 2502.09819) [V]:
  - GPT-4o; success rates: AIDL 64%, without constraints 94%, without hierarchy 77%, OpenSCAD 79%.
  - The language's richer structure lowered validity while raising quality: CLIP 28.90 vs 27.32 [A].
- **Pipelex** [A]: TOML-based declarative workflows. Elastic License 2.0, 939★. No evaluation.

## Empirical evidence on language properties

Each row: the claim, the number, the source, and what it means for LotML. Rows already in the wiki are
listed at the end of each subsection, not repeated.

### Static types and annotations

| claim | number | source | applicability to LotML |
| --- | --- | --- | --- |
| Compile-time checks contain a weaker model's errors | fix-commit ratio, Opus → GLM: TS 41.7% → 63.3%, Python 48.4% → 86.8% (one developer, one monorepo, 28 days each) | [Peng et al. 2607.13080](https://arxiv.org/abs/2607.13080) [V] | Weak (n=1 developer), but it is the open-model case where LotML's checker matters most |
| Untyped failures surface late | Python: 13 runtime wrong answers vs 3 compile rejections; TS: 1 and 1 | [VeraBench](https://github.com/aallan/vera-bench) [V] | Supports reporting "refused by `lotml check`" apart from "wrong answer" |
| Full annotations cost agent time on small tasks | Python 74.6 s / $0.38 vs Python + `mypy --strict` 125.3 s / $0.57; Ruby + Steep 2.0–3.2× slower; 600 runs, Opus 4.6, all passing | [mame/ai-coding-lang-bench](https://github.com/mame/ai-coding-lang-bench) [V] | LotML's token target is "no worse than *typed* Python"; against untyped Python it will pay this tax. Its design (same language with and without a checker) is the ablation LotML's harness lacks |
| Type-directed decoding removes compile errors and raises pass@1 (small models) | SuFu, 2B model: pass@1 29.31% → 43.10%, compile errors 61.21% → 0.00% | [TyFlow 2510.10216](https://arxiv.org/abs/2510.10216) [V] | Supports types checkable on prefixes; small models only |
| Stating I/O types is a top fix for failing prompts | used in 44% of 627 successful prompt rewrites (algorithm details 57%, exceptions 12%) | [Midolo et al. 2601.13118](https://arxiv.org/abs/2601.13118) [A] | Mandatory signatures put this in the code |
| TS vs JS has no consistent sign | SWE-PolyBench (Sonnet 3.5): Aider TS 13.0 vs JS 12.6; SWE-agent 10.2 vs 6.5; Agentless 4.7 vs 7.2 | [SWE-PolyBench 2504.08703](https://arxiv.org/abs/2504.08703) [V] | Annotations alone are not the lever; the check loop is |

Already in the wiki: typed holes (types in context multiply results; `semantic-compiler`); MultiPL-E's
annotation-removal finding; type-constrained decoding and its replication.

### Accuracy gaps between languages

| claim | number | source | applicability |
| --- | --- | --- | --- |
| Popularity predicts success | 3,011 LeetCode problems × 9 languages × 5 models; DeepSeek-V3 Python 79.81 vs Erlang 24.31 vs Racket 20.82; mainstream–niche gap 28.9–44.8 points [A] | [Matthew Effect 2509.23261](https://arxiv.org/abs/2509.23261) [V] | LotML starts at the niche end unless Python transfers (adr:0010) |
| Same tasks, 12 languages: small gap for typed mainstream | mean pass@1 Python 0.482, Java and C++ ≈ 0.44; "Python overfitting" and language-specific contamination | [Multi-LCB 2606.20517](https://arxiv.org/abs/2606.20517) (ICLR 2026) [V] | Only aligned-task data here; use it rather than AutoCodeBench |
| Repository work: the strictest compiler leads outside Python | SWE-bench Multilingual, Claude 3.7 + SWE-agent: Rust 58.14%, Java 53.49%, PHP 48.84%, Ruby 43.18%, JS/TS 34.88%, Go 30.95%, C/C++ 28.57%; Verified 63% | [swebench.com/multilingual](https://www.swebench.com/multilingual.html) [V] | Compile feedback can offset less data; different repos per language, so not controlled |
| Repository work: Python far ahead | Multi-SWE-bench, Claude 3.7, MopenHands: Python 52.2, Java 21.88, TS 2.23, JS 5.06, Go 7.48, Rust 15.90, C 8.59, C++ 14.73 (Python column is SWE-bench Verified) | [Multi-SWE-bench 2504.02605](https://arxiv.org/abs/2504.02605) [V] | Same caution |
| Per-language problems flip rankings | Claude Opus 4 (reasoning): Elixir 80.3, C# 74.9, Kotlin 72.5, Python 40.3, Rust 38.7; problems generated and filtered per language | [AutoCodeBench 2508.09101](https://arxiv.org/abs/2508.09101) [A, read twice] | Not a language effect. Valim's "Why Elixir is the best language for AI" rests on it ([dashbit](https://dashbit.co/blog/why-elixir-best-language-for-ai) [V]) |
| Agents can build large systems in a new typed language | SWE-AGI (MoonBit, 1k–10k-line systems): gpt-5.3-codex 19/22 (86.4%), claude-opus-4.6 15/22 (68.2%) | [SWE-AGI 2602.09447](https://arxiv.org/abs/2602.09447) [V] | Frontier agents plus spec plus toolchain suffice at scale; no comparison language |
| Rust repository issues are hard | Rust-SWE-bench, 500 tasks: OpenHands + Claude 3.7 21.2%, RustForger 28.6%; failures: repository structure 43.7%, type/trait system 32.6% [A] | [2602.22764](https://arxiv.org/abs/2602.22764) [V] | Rich type systems cost at repository scale |
| Aider polyglot publishes no per-language rates | 225 exercises, 6 languages | [aider.chat](https://aider.chat/2024/12/21/polyglot.html) [A] | Cannot be cited for language gaps |

Already in the wiki: MultiPL-E, McEval-Hard (Gleam/MoonBit), Moumoula's language confusion, Tokenmaxxing.

### Verbosity and token efficiency

| claim | number | source | applicability |
| --- | --- | --- | --- |
| More explicit structure can raise accuracy | Grammar-rule continued pretraining vs tokens, HumanEval: 1.3B 63.4 vs 43.9; 1.5B 65.9 vs 50.6; 7B 76.8 vs 68.9 | [2503.05507](https://arxiv.org/abs/2503.05507) [V] | Counter-evidence to compressing syntax; small models |
| Fewer lines ≠ cheaper agent | OCaml 216 lines $0.58 vs Ruby 219 lines $0.36 | [mame](https://github.com/mame/ai-coding-lang-bench) [V] | Familiarity beats terseness |
| Fine-tuned terse syntax can keep correctness | ShortCoder: −18.1% tokens on HumanEval "without functional compromise" | [2601.09703](https://arxiv.org/abs/2601.09703) [A] | Requires training; same band as Token Sugar |
| Exotic languages cost far more agent money | 17 languages, 34 chess engines: about $2–30 mainstream vs $60–480 exotic | [Acher, Jézéquel 2606.13763](https://arxiv.org/abs/2606.13763) [U] (tool-extracted) | Cost follows the prior |

### Indentation versus braces

| claim | number | source | applicability |
| --- | --- | --- | --- |
| Python patches fail on indentation | 13 LLMs, ~195k full-function patches over 4 benchmarks; Python's poor results are "largely due to indentation issues in the generated patches"; Gemini 2.0 Flash the exception | [Campos et al. 2506.03283](https://arxiv.org/abs/2506.03283) [V] | The failure LotML's symbol-addressed, re-indenting edits target (adr:0010) |
| Indentation is a minor error class in fresh code | Of 151 errors in generated Python tests: AssertionError 64.9%, IndentationError 11.3%, SyntaxError 4.6% | [Alves et al. 2506.14297](https://arxiv.org/abs/2506.14297) [A] | Secondary in generation; primary in patching |
| Formatting perturbations matter less than semantic ones | 29 perturbations, 6 of them formatting; Java, C++, JS only | [Rabbi et al. 2504.19108](https://arxiv.org/abs/2504.19108) [A] | No Python, no direct comparison |

**Gap:** no published controlled comparison of the same language with indentation and with
braces. LotML's phase 0 gate (192 paired editing tasks, adr:0010) appears to be the only one, and is
worth publishing as such.

### Exceptions versus result types

| claim | number | source | applicability |
| --- | --- | --- | --- |
| Models barely handle exceptions unprompted | 750 fragile Java snippets: general prompting 13% coverage vs Seeker 91% coverage, 79% accuracy; up to 62.91% of human exception blocks have violations | [Seeker 2412.11713](https://arxiv.org/abs/2412.11713) [V] (13% [A]) | `T ! E` makes the error path a signature obligation, not an afterthought |
| Exception handling is a stable share of LLM bugs | SonarQube on 4,442 Java tasks: 16.75% (Claude Sonnet 4), 16.71% (Claude 3.7), 11.60% (GPT-4o), 14.39% (Llama 3.2 90B), 14.52% (OpenCoder-8B); "catches overly broad exceptions" | [Sonar 2508.14727](https://arxiv.org/abs/2508.14727) [V] | Target for adr:0002 |
| Stated preconditions are ignored | 364 tasks: pass@1 75–82% with 0% contract satisfaction; 23–41% when stated in the prompt | [ContractEval 2510.12047](https://arxiv.org/abs/2510.12047) [V] | Checks must live in types or contracts, not prose |

**Gap:** no study compares `Result`/`err` with exceptions in LLM-written code. LotML's phase 1 data could
be split into fallible and infallible tasks to give the first answer.

### Compile-error taxonomies (beyond the wiki)

- **Name resolution leads.** In Rust repository issues, E0599 (method not found) 18.06%, E0433 16.21%,
  E0432 12.08%, E0425 8.54%, E0308 6.81%, E0277 6.50% [A]
  ([2602.22764](https://arxiv.org/abs/2602.22764)). This matches the wiki's RustRepoTrans finding.
- **The compiler loop closes much of the gap.** For a low-resource language (COBOL), compiler-feedback
  repair took compile success from 41.8% to 95.89% with GPT-4o [A]
  ([2604.03978](https://arxiv.org/abs/2604.03978)).
- **A new language's API is post-cutoff by definition.** On API changes, models succeed 56.1% before
  their cutoff against 32.5% after, and retrieval adds about 13.5% [A]
  ([RustEvo² 2503.16922](https://arxiv.org/abs/2503.16922)). Ship standard-library documentation for
  retrieval.

### Intermediate languages: the model writes something familiar, a compiler lowers it

| approach | number | source | applicability |
| --- | --- | --- | --- |
| Quasar: Python subset, transpiled, repaired from static errors | AgentDojo execution 76.3 (Python) → 82.8 (subset) → 89.2 (multi-turn static repair); accuracy 64.5 → 63.4 → 67.7 | [2506.12202v2](https://arxiv.org/abs/2506.12202) [V] | A checker over a Python-shaped language beats unchecked Python: LotML's thesis in miniature |
| Dafny as a verification-aware IL, compiled to Python | HumanEval, Claude 3.5 Sonnet: converged on 144 of 164, 127 passed (77%) vs 86% writing Python directly; ~88% with fallback | [2501.06283](https://arxiv.org/abs/2501.06283) [V] | A verification layer on a Python target cost 9 points without fallback |
| LLMLift: Python as IR plus proof, lowered to DSLs | Spark 44/45, TACO 60/60, tensor IR 23/23, Domino 10/10 | [2406.03003](https://arxiv.org/abs/2406.03003) [A] | Python-shaped IR works when a checker exists |
| InterTrans: translation through intermediate languages | +18.3 to +43.3 points computational accuracy over direct translation | [2411.01063](https://arxiv.org/abs/2411.01063) [A] | Relevant to the Python→LotML corpus pipeline |
| Djinnlang: specifications compiled by an LLM into verified Dafny | 256/256 obligations; Codex $47 vs Claude Code $1,026 | [2609.23954](https://arxiv.org/abs/2609.23954) [V] | Model-in-compiler is viable only with a verifier as judge |

Already in the wiki: SPEAC, PyLang, MultiPL-T.

### Verification-oriented targets

| claim | number | source |
| --- | --- | --- |
| Vericoding across languages | 12,504 specs (3,029 Dafny, 2,334 Verus, 7,141 Lean); union of off-the-shelf models: Lean 27%, Verus 44%, Dafny 82%; NL descriptions "do not significantly improve"; pure Dafny verification rose from 68% to 96% in a year | [2509.22908](https://arxiv.org/abs/2509.22908) [V] |
| Same 77 algorithms, identical contracts | Gemini-3 Flash: Dafny 40.3%, Verus 24.7%, Lean 7.8% (after a fidelity filter); GPT-5.3 Codex before the filter 49.35 / 14.29 / 23.38 [A] | [AlgoVeri 2602.09464](https://arxiv.org/abs/2602.09464) [V] |
| Python with contracts (Nagini) trails Dafny | Claude 3.5 Sonnet, verified in at least one run, Dafny / Nagini / Verus: proof only 86/66/45%; from NL plus spec functions 61/42/24%; NL only 29/15/15% | [Shefer et al. 2503.14183](https://arxiv.org/abs/2503.14183) [V] |
| End to end from NL is near the floor | o3-mini 3.62% zero-shot, 9.37% after refinement | [VerifyThisBench 2505.19271](https://arxiv.org/abs/2505.19271) [V] |
| Agentic Dafny | DafnyBench 725/782 (92.7%), +6.5 points | [AxDafny 2606.32007](https://arxiv.org/abs/2606.32007) [V] |
| Lean: code easy, proofs hard | o3: code 72.6%, specs 52.3%, proofs 4.9% | [Verina 2505.23135](https://arxiv.org/abs/2505.23135) [V] |
| Lean vericoding with tools | GPT-5.4 95.0% on 423 specs, K=50 calls, mathlib search | [2605.27485](https://arxiv.org/abs/2605.27485) [V] |
| Repository-scale Lean | best agent fully solves 27 of 43 repositories | [Vero 2608.13522](https://arxiv.org/abs/2608.13522) [V] |
| Verus proof synthesis | AutoVerus 137/150 (91.3%) vs GPT-4o 67/150; VeruSAGE >80% on 849 system tasks | [2409.13082](https://arxiv.org/abs/2409.13082), [2512.18436](https://arxiv.org/abs/2512.18436) [A] |
| Spec hacking under RL | reward 2.2% → 58.1% with weak specs; 9.7% → 31.1% after filtering | [Tan 2605.30914](https://arxiv.org/abs/2605.30914) [A] |

Why the ranking holds is contested:
- **Design.** Dafny has SMT automation and uniform mathematical types. Verus forces machine integers and
  ghost/native distinctions. Lean needs explicit proof terms.
- **Data.** Lean data is mostly mathematics; Dafny has more verification code.

AlgoVeri controls the problems but not the data. Either way, a contract layer on a Python-shaped language
(Nagini) did not inherit Python's prior.

### Which language models choose

- **Models default to Python.** It is 90–97% of benchmark solutions, and 58% of high-performance projects
  where it is a poor fit; Rust was never chosen [V]
  ([Twist et al. 2503.17181](https://arxiv.org/abs/2503.17181)).
- **They implement in Python more than they recommend it.** 25 models: Python in 35.3% of
  implementations against 10.7% of recommendations; 69.8% of Python choices made with no deliberation [V]
  ([LangChoiceBench 2608.06041](https://arxiv.org/abs/2608.06041)).
- **Language families predict transfer** between related languages [A]
  ([2512.19509](https://arxiv.org/abs/2512.19509), FSE 2026). Python-shaped syntax maximises it.

## Languages for programming with LLMs (secondary)

Only methodology and measured results; all [A] unless marked.

| system | method | measured result | source |
| --- | --- | --- | --- |
| BAML (SAP) | schema-aligned parsing (least-cost edit to the schema) vs function calling, on BFCL, n=1000 per model, vendor-run | gpt-4o: function calling 87.4, Python AST parser 82.1, SAP 93; gpt-4o-mini 19.8 / 51.8 / 92.4; claude-3-haiku 57.3 / 82.6 / 91.7. Dec 2025 post: gpt-4o 93.63% with BAML vs 91.37% with constrained decoding | [boundaryml.com/blog/schema-aligned-parsing](https://www.boundaryml.com/blog/schema-aligned-parsing), [structured-outputs-create-false-confidence](https://boundaryml.com/blog/structured-outputs-create-false-confidence) |
| LMQL | constrained, scripted prompting vs plain prompting | keeps or raises accuracy; 26–85% cost savings | [2212.06094](https://arxiv.org/abs/2212.06094) |
| Jac / byLLM (MTP) | `by` operator; 13 tasks vs LMQL and DSPy; user study with 20 students | 3.2× faster with 45% fewer lines (user study); cost up to 4.5× lower and speed up to 4.75× higher than DSPy [V] | [2405.08965](https://arxiv.org/abs/2405.08965) |
| DSPy | signatures plus optimizer; GSM8K and multi-hop QA | beats few-shot by >25% (GPT-3.5) and >65% (llama2-13b) | [2310.03714](https://arxiv.org/abs/2310.03714) |
| SGLang | frontend primitives plus RadixAttention runtime | up to 6.4× throughput | [2312.07104](https://arxiv.org/abs/2312.07104) |
| POML | markup separating content from style; TableQA over 73,926 style configurations | GPT-3.5 accuracy 6% → 61.8% depending on style alone | [2508.13948](https://arxiv.org/abs/2508.13948) |
| APPL | prompts in Python with an async runtime | CoT-SC 9.49× measured vs 10× ideal speedup; programs 1.6–2.2× smaller than LMQL, SGLang and Guidance | [2406.13161](https://arxiv.org/abs/2406.13161) |
| AutoPDL | prompt-language configuration search | +9.21 ± 15.46 points, up to 67.5 | [2504.04365](https://arxiv.org/abs/2504.04365) |
| LOTUS | semantic operators over dataframes | optimisations up to 1,000×; quality up to +170% | [2407.11418](https://arxiv.org/abs/2407.11418) |
| Turn | compiled actor language with "cognitive type safety" | evaluation claimed, no numbers in the abstract [V]; MIT, 12★ | [2603.08755](https://arxiv.org/abs/2603.08755) |

Guidance's measured results are JSONSchemaBench, already in the wiki. Lesson for LotML: in this family,
the strongest gains come from structure the runtime enforces (parsing to types, schemas, caching), not
from surface syntax. The same pattern as the code-writing languages.

## Reference repositories worth studying

| repository | license | why |
| --- | --- | --- |
| [aallan/vera](https://github.com/aallan/vera) | MIT | Diagnostics written as model instructions with stable codes; mandatory contracts with an SMT fallback to runtime checks; effect rows; SKILL.md packaging. The closest design analogue. |
| [aallan/vera-bench](https://github.com/aallan/vera-bench) | MIT | Ready-made cross-language harness (Python, TS, three zero-data languages); check@1 / verify@1 / fix@1 metrics; candid saturation analysis. Its 60 problems could be posed in LotML. |
| [jasisz/aver](https://github.com/jasisz/aver) | MIT | Rust toolchain; `agent-connect` ships the language guide inside the binary; `decision` / `verify` blocks; record/replay. |
| [jasisz/intent-trace](https://github.com/jasisz/intent-trace) | none declared | Methodology: the Aver-in-Python control separates "format" from "language". |
| [almide/almide](https://github.com/almide/almide) · [almide-dojo](https://github.com/almide/almide-dojo) | MIT/Apache-2.0 · MIT | Rust-hosted, Perceus-style RC with a Coq-checked checker, native plus Wasm, the "modification survival rate" metric, CI-enforced claim discipline. |
| [mame/ai-coding-lang-bench](https://github.com/mame/ai-coding-lang-bench) | none declared | 13 languages × 20 runs with the same-language typed/untyped ablation; LotML could be added as a column (data only, no code reuse without a license). |
| [sunholo-data/ailang](https://github.com/sunholo-data/ailang) | Apache-2.0 | MCP tool set and Claude Code / Codex plugin packaging to compare with LotML's MCP server. |
| [vercel-labs/zerolang](https://github.com/vercel-labs/zerolang) | Apache-2.0 | Stable diagnostic codes with fix plans; hash-guarded `query`/`patch` graph edits; an unpublished evals harness. |
| [BoundaryML/baml](https://github.com/BoundaryML/baml) | Apache-2.0 | Schema-aligned parsing; `baml describe` for AST-aware discovery; colorless concurrency. |
| [stephenmell/quasar-colm2026-artifact](https://github.com/stephenmell/quasar-colm2026-artifact) | none declared | Python-subset transpiler with a static-error repair loop. |
| [microsoft/flint-chart](https://github.com/microsoft/flint-chart) | MIT | An IL served over MCP, with an evaluation reported as paired win/tie/loss with p-values. |
| [secure-foundations/human-eval-verus](https://github.com/secure-foundations/human-eval-verus), [JetBrains-Research/HumanEval-Nagini](https://github.com/JetBrains-Research/HumanEval-Nagini), [Beneficial-AI-Foundation/vericoding](https://github.com/Beneficial-AI-Foundation/vericoding) | MIT, Apache-2.0, MIT | Task sets if LotML ever adds contracts. |

## Implications for LotML

1. **The measured slot is now contested, and LotML's method is the stronger one.**
   - VeraBench, intent-trace and Almide's dojo all measure "a language for models". All three use a
     single run, have no significance tests, and VeraBench is saturated.
   - LotML's gates are paired with exact McNemar tests (200 paired tasks; 192 for editing), include
     7–8B models, and recorded failures (adr:0011, adr:0019).
   - Two cheap additions would place LotML in that comparison:
     - pose VeraBench's 60 problems in LotML (MIT);
     - add LotML as a column in mame's MiniGit benchmark.
2. **Python's prior is the right bet (adr:0010).** Nothing found contradicts "Python syntax where
   semantics match": the Matthew effect, Multi-LCB, LangChoiceBench and the language-family transfer
   study all support it. VeraBench shows a frontier model with a skill file can master an alien design on
   small problems. That matches phase 1 (Sonnet at parity) and says nothing for the 7–8B models where
   LotML failed; Almide's 8B score of 44% is the same pattern.
3. **Indentation (adr:0010).** No conflict.
   - The one measured harm is Python *patches* misaligned in context (Campos et al.). That is exactly
     what LotML's symbol-addressed edits, tolerant parser and canonical formatter are for.
   - Requirement it sharpens: whole-function replacement through the MCP/LSP edit path must re-indent to
     the target, and the agent harness should route edits through it rather than generic string
     replacement.
4. **Errors as values (adr:0002).** No conflict and no direct evidence either way. Exception handling is
   11.6–16.75% of LLM Java bug findings, and stated preconditions are 0% enforced unprompted. Splitting
   phase-1 results by fallible and infallible tasks would be the first published comparison of result
   types and exceptions.
5. **The type-checking tax (non-ADR tension).**
   - mame measured full annotations at 1.6–1.7× agent time on a small task. Valim argues the opposite
     direction ("making types and intentions explicit gives the compiler… more information").
   - LotML's "explicit at the boundary, inferred inside" sits between them. The typed/untyped Python
     ablation belongs in the harness so the tax is measured, not assumed.
   - adr:0015 (pose HumanEval untyped, count signature refusals apart) is consistent with VeraBench's
     "Vera NL" split. Writing contracts oneself cost 0–10 points there.
6. **If contracts or effects are ever added** (no ADR yet):
   - Make them Dafny/Vera-style: SMT-discharged, with undecidable obligations becoming runtime checks.
     Proof-heavy designs score far lower (Lean 7.8% on AlgoVeri).
   - Expect a pass-rate cost: Dafny-as-IL lost 9 points against direct Python. Keep a fallback.
   - Expect specification hacking (wiki `reward-hacking`). A contract on a Python-shaped language does
     not inherit Python's prior (Nagini trails Dafny).
7. **Diagnostics and packaging have converged.**
   - Stable codes, an `explain` command, a concrete fix and a spec reference per diagnostic are now the
     norm (Vera, zerolang, Codong, Almide).
   - Shipping the language guide from the binary as a skill or `llms.txt`, version-matched, is too (Aver
     `agent-connect`, AILANG `ailang prompt`, BAML `agent install`, Almide's frozen surface).
   - Zero's caveat is worth copying: JSON where an agent parses, not by default everywhere.
8. **Query the program, not positions.**
   - Valim, zerolang (`query`/`patch` guarded by hashes), BAML `describe` and Aver `context` all expose
     the program as a database.
   - This agrees with adr:0009/0010's symbol-addressed edits and suggests a symbol-query tool in LotML's
     MCP server. No ADR conflict.
9. **Wiki corrections to carry over:**
   - Anka's +40 is the 3–5-operation category (the figure says 5+).
   - MoonBit's sampler numbers (43.75% → 56.25%, CodeLlama-34B, 32 effective tasks) and the missed 1.0.
   - Quasar v2's AgentDojo results, including the static-repair gain over Python.
   - Zero renamed zerolang, dormant since June.
   - NanoLang's tests are no longer mandatory everywhere.
10. **Naming note.** LOTUS (Stanford, arXiv 2407.11418) is an established LLM-programming system. It is a
    search-collision risk for LotML's "Lotus" identity (adr:0018 fixes only the `.lot` extension). Minor.

**Conflicts with accepted ADRs: none found.** The closest is opinion, not data: Ronacher's essay and the
B-IR author both dislike significant whitespace, against adr:0010. The only data for that dispute remains
LotML's own gate.

## Sources

Papers (arXiv abstract pages unless noted):
- Quasar https://arxiv.org/abs/2506.12202 (PDF v2 https://arxiv.org/pdf/2506.12202v2) · artifact https://github.com/stephenmell/quasar-colm2026-artifact
- Anka https://arxiv.org/abs/2512.23214 (PDF v1) · https://github.com/BleBlo/Anka
- Pel https://arxiv.org/abs/2505.13453
- MoonBit, LLM4Code 2024 https://llm4code.github.io/2024/assets/pdf/papers/7.pdf (DOI 10.1145/3643795.3648376)
- SWE-AGI https://arxiv.org/abs/2602.09447
- Djinnlang https://arxiv.org/abs/2609.23954
- Turn https://arxiv.org/abs/2603.08755
- Flint https://arxiv.org/abs/2607.20775 · AIDL https://arxiv.org/abs/2502.09819
- Peng et al. https://arxiv.org/abs/2607.13080 · TyFlow https://arxiv.org/abs/2510.10216 · Midolo et al. https://arxiv.org/abs/2601.13118
- Matthew Effect https://arxiv.org/abs/2509.23261 · Multi-LCB https://arxiv.org/abs/2606.20517 · AutoCodeBench https://arxiv.org/abs/2508.09101 (HTML v1)
- Multi-SWE-bench https://arxiv.org/abs/2504.02605 · SWE-PolyBench https://arxiv.org/abs/2504.08703 · Rust-SWE-bench https://arxiv.org/abs/2602.22764
- Grammar-based representation https://arxiv.org/abs/2503.05507 · ShortCoder https://arxiv.org/abs/2601.09703 · Acher and Jézéquel https://arxiv.org/abs/2606.13763
- Campos et al. https://arxiv.org/abs/2506.03283 · Alves et al. https://arxiv.org/abs/2506.14297 · Rabbi et al. https://arxiv.org/abs/2504.19108
- Seeker https://arxiv.org/abs/2412.11713 · Sonar https://arxiv.org/abs/2508.14727 · ContractEval https://arxiv.org/abs/2510.12047
- COBOLAssist https://arxiv.org/abs/2604.03978 · RustEvo² https://arxiv.org/abs/2503.16922
- Dafny as IL https://arxiv.org/abs/2501.06283 · LLMLift https://arxiv.org/abs/2406.03003 · InterTrans https://arxiv.org/abs/2411.01063
- Vericoding https://arxiv.org/abs/2509.22908 · AlgoVeri https://arxiv.org/abs/2602.09464 · Shefer et al. https://arxiv.org/abs/2503.14183
- VerifyThisBench https://arxiv.org/abs/2505.19271 · AxDafny https://arxiv.org/abs/2606.32007 · Verina https://arxiv.org/abs/2505.23135
- Lean agent search https://arxiv.org/abs/2605.27485 · Vero https://arxiv.org/abs/2608.13522 · AutoVerus https://arxiv.org/abs/2409.13082
- VeruSAGE https://arxiv.org/abs/2512.18436 · Tan https://arxiv.org/abs/2605.30914
- LLMs Love Python https://arxiv.org/abs/2503.17181 · LangChoiceBench https://arxiv.org/abs/2608.06041 · Language families https://arxiv.org/abs/2512.19509
- LMQL https://arxiv.org/abs/2212.06094 · MTP https://arxiv.org/abs/2405.08965 · DSPy https://arxiv.org/abs/2310.03714
- SGLang https://arxiv.org/abs/2312.07104 · POML https://arxiv.org/abs/2508.13948 · APPL https://arxiv.org/abs/2406.13161
- AutoPDL https://arxiv.org/abs/2504.04365 · LOTUS https://arxiv.org/abs/2407.11418

Repositories and official pages (READMEs read through `gh api`):
- https://github.com/aallan/vera · https://github.com/aallan/vera-bench · https://veralang.dev
- https://github.com/jasisz/aver · https://github.com/jasisz/intent-trace
- https://github.com/sunholo-data/ailang · https://ailang.sunholo.com/docs/benchmarks/performance
- https://github.com/almide/almide · https://github.com/almide/almide-dojo
- https://github.com/voltropy/mog · https://github.com/jbwinters/jacquard-lang · https://github.com/Nerd-Lang/nerd-lang-core
- https://github.com/brettinhere/Codong · https://github.com/TakatoHonda/sui-lang
- https://github.com/ImJasonH/ImJasonH/blob/main/articles/llm-programming-language.md · https://github.com/Tubifix77/weft
- https://codespeak.dev · https://github.com/PLangHQ/plang · https://github.com/alantech/marsha · https://github.com/Pipelex/pipelex
- https://github.com/microsoft/flint-chart · https://github.com/mame/ai-coding-lang-bench
- https://github.com/BoundaryML/baml · https://www.boundaryml.com/blog/schema-aligned-parsing · https://boundaryml.com/blog/structured-outputs-create-false-confidence
- https://github.com/moonbitlang/core · https://www.moonbitlang.com/blog/
- https://github.com/jordanhubbard/nanolang · https://simonwillison.net/2026/Jan/19/nanolang/
- https://github.com/vercel-labs/zerolang · https://zerolang.ai
- https://www.swebench.com/multilingual.html · https://aider.chat/2024/12/21/polyglot.html
- https://dashbit.co/blog/evolving-ai-era · https://dashbit.co/blog/why-elixir-best-language-for-ai

Hacker News threads (criticism only):
- Mog https://news.ycombinator.com/item?id=47312728 · Jacquard https://news.ycombinator.com/item?id=48894630
- NERD https://news.ycombinator.com/item?id=46450217 · B-IR https://news.ycombinator.com/item?id=46583581
- CodeSpeak https://news.ycombinator.com/item?id=47350931 · Flint https://news.ycombinator.com/item?id=48834924
