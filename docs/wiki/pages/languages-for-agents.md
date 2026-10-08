# Languages for agents

The original study states that no language has been designed for LLMs. By October 2026 that no
longer holds: there are several, all recent. This page records what each one decided, where each
one stands, and what the whole family has in common — and what none of them has: measured
evidence that the design helps the model.

## Languages that call themselves built for agents

| language | what it decided for LLMs | status |
| --- | --- | --- |
| [BAML](https://github.com/BoundaryML/baml) | "the programming language for agents"; looks like TypeScript; types persist at runtime, no `any`; typed, statically analyzed errors; built-in tests and evals; colorless concurrency with green threads; callable from TS, Python, Go, C#, Java | pre-1.0, nightly builds |
| MoonBit | "flattened design": mandatory signatures at module level and separate local definitions, for linear generation; a sampler that resamples on syntax and on types with backtracking (its LLM4Code 2024 paper gives 43.75% against 56.25% compile rate for CodeLlama-34B on about 33 small tasks, with a ~3% throughput cost); expect tests; `moon prove` for formal verification since 0.9 | 1.0 was planned for the first half of 2026 and has not shipped |
| [Pel](https://arxiv.org/abs/2505.13453) | minimal, homoiconic grammar meant for constrained generation; capability control in the syntax; natural-language conditions evaluated by an LLM | paper with no empirical evaluation |
| [Quasar](https://arxiv.org/abs/2506.12202) | separates internal logic from tool calls with effect annotations, for access control and parallelization; the model writes a restricted Python subset that a transpiler compiles to Quasar, because models struggle to write Quasar directly | COLM 2026; measured: the subset kept accuracy close to unrestricted Python (71.4 against 71.8), and the annotations cut approvals by 26% and running time by 18–27% |
| [Anka](https://arxiv.org/abs/2512.23214) | a data-pipeline DSL: one canonical form per operation and a mandatory, uniquely named result for every step | arXiv 2025; learned from the prompt alone (99.9% parse, 95.8% tasks with Claude 3.5 Haiku); +40 points over Python on its multi-step category, which its Table 2 defines as 3–5 operations (only a figure says 5+), none on short tasks |
| [NanoLang](https://github.com/jordanhubbard/nanolang) | "designed for machines to write and humans to read"; test blocks, no longer enforced everywhere ("missing-shadow enforcement is not universal"); a JSON spec for the model | v5.0.0; no measured results |
| Zero, now zerolang (Vercel Labs) | started with JSON diagnostics with stable codes and `zero fix --plan --json`; the current README turned "graph-native", with the program as a semantic database edited through `zero query` and `zero patch`; its evals harness has published no results | experimental; last release v0.3.4 (June 2026), quiet since |

## Precedents by aspect

The languages the original study lists as references still hold, each for one aspect; their 2026
status is on each topic's page.

- **Python-derived syntax with native performance:** Mojo (1.0 on 2026-08-11, compiler
  open-sourced under Apache 2.0 on 2026-08-18), Codon, Cython — see [[transpilation-strategy]].
- **Value semantics and reference counting:** Swift, Hylo, Lobster, Koka, Lean 4, Roc, Nim — see
  [[memory-model]].
- **Colorless concurrency:** Go, Java virtual threads, Zig, BEAM — see [[colorless-concurrency]].
- **Effects:** Koka, Effekt, Unison, Flix — see [[type-system]].
- **Diagnostics and the compiler as a tool:** Rust, Elm, gopls with built-in MCP — see
  [[semantic-compiler]].
- **Grammars for decoding:** tree-sitter, GBNF, llguidance — see [[constrained-decoding]].

## Practitioner essays

- **Armin Ronacher, *A Language For Agents*** (blog, 2026-02-09): "whitespace-based indentation
  is a problem"; agents "are afraid of" exceptions and prefer typed results; argues for explicit
  effects, local reasoning and names findable by text search.
- **Haupt, *Markov*** (2026): sum types with exhaustive `match` and compiler errors "phrased as
  prompts with suggestions… formatted as diffs".

Opinion, not data — but they agree with the evidence in [[semantic-compiler]] on editing and
diagnostics.

## What the family has in common

Typed errors or results instead of exceptions, explicit effects or capabilities, built-in tests
and machine-readable tooling. The lotml proposal converges with all of them.

**Almost none has published evidence that its design improves model accuracy.** Quasar measured
tooling benefits but has the model write Python, not Quasar. Anka is the exception, and a weak one:
same tasks and prompt structure, Anka above Python (95.8% against 91.2% overall), with the gain
entirely on long pipelines where Python's failures were mostly variable shadowing (42%) — reused
names like `df` or `result` — which Anka's unique step names forbid. Its prompt carried a 100-line
syntax guide the Python prompt did not, its principles were never ablated one by one, and it covers
only data pipelines. The lesson that transfers to lotml is narrow and consistent with the rest of
the evidence: one canonical form per operation, and no silent reuse of a name — which lotml's
immutable-by-default locals already enforce.

## Since the first survey

By October 2026 nine more languages were built for models to write — Vera, Aver, AILANG,
Almide, Mog, Jacquard, NERD, Codong and Sui — and four more have a compiler that calls a model
(CodeSpeak, Djinnlang, Plang, Marsha). Three now publish measurements:

- **Vera** (MIT): no variable names, mandatory contracts checked by Z3, effect rows, and
  diagnostics written as instructions to the model. VeraBench, 60 problems across nine frontier
  models, one run each: Vera 98.7%, Python 96.7%, TypeScript 99.7%. The benchmark is saturated
  and has no significance tests.
- **Aver** (MIT): its intent-trace study, about 19,000 judgments, compared Aver, Aver
  transliterated into Python, and idiomatic Python. Aver and its Python transliteration tied;
  idiomatic Python came out below. The structure carried the legibility, not the syntax: the
  ablation the rest of the family skips.
- **Almide** (MIT/Apache-2.0): the nearest cousin to lotml. It has a compiler written in Rust,
  reference counting with reuse, native and Wasm targets, and a metric of its own, the
  "modification survival rate". On its 38-task dojo, Llama 3.3 70B scored 65% and Llama 3.1 8B
  44%, with no other language run on it.

Quasar's v2 adds a feedback-loop result: with multi-turn repair, 89.2% of its AgentDojo programs
ran without error against 76.3% for single-shot Python, while accuracy moved from 64.5% to 67.7%
on 93 tasks, and the subset alone left accuracy unchanged (63.4% against 64.5%). None of them
has lotml's paired design with exact tests, or measures 7–8B models. The evidence by language
property is in [[language-design-evidence]]; the sources are in
`research/llm-landscape/languages.md`.

That leaves the space this project's [[evaluation-harness]] can occupy: the differentiator is not
having the same features, it is measuring which of them matter.
