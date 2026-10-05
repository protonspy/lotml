# Languages for agents

The original study states that no language has been designed for LLMs. By October 2026 that no
longer holds: there are several, all recent. This page records what each one decided, where each
one stands, and what the whole family has in common — and what none of them has: measured
evidence that the design helps the model.

## Languages that call themselves built for agents

| language | what it decided for LLMs | status |
| --- | --- | --- |
| [BAML](https://github.com/BoundaryML/baml) | "the programming language for agents"; looks like TypeScript; types persist at runtime, no `any`; typed, statically analyzed errors; built-in tests and evals; colorless concurrency with green threads; callable from TS, Python, Go, C#, Java | pre-1.0, nightly builds |
| MoonBit | "flattened design": mandatory signatures at module level and separate local definitions, for linear generation; a sampler that resamples on syntax and on types with backtracking (compile-rate gain announced without absolute numbers); expect tests | v0.10.x in 2026, 1.0 not yet released |
| [Pel](https://arxiv.org/abs/2505.13453) | minimal, homoiconic grammar meant for constrained generation; capability control in the syntax; natural-language conditions evaluated by an LLM | paper with no empirical evaluation |
| [Quasar](https://arxiv.org/abs/2506.12202) | separates internal logic from tool calls with effect annotations, for access control and parallelization | COLM 2026 |
| [NanoLang](https://github.com/jordanhubbard/nanolang) | "designed for machines to write and humans to read"; mandatory test blocks; a JSON spec for the model | no measured results |
| Zero (Vercel Labs) | started with JSON diagnostics with stable codes and `zero fix --plan --json`; the current README turned "graph-native", with the program as a semantic database edited through `zero query` and `zero patch` | experimental, "expect breaking changes" |

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

**None has published controlled evidence that its design improves model accuracy.** That is the
space this project's [[evaluation-harness]] can occupy: the differentiator is not having the same
features, it is measuring which of them matter.
