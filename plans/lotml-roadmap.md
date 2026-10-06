---
autonomy: auto
ci: wait
status: approved
pr: per-group
merge: manual
checksum: a081260f99b7f51b3a85dad8195eea6f49a7f3c7e038b91a7f59ce96e9707a98
---

# lotml roadmap

The complete path from today's research to lotml: five phases, each closed by a measured gate,
every task naming the evidence it rests on. Phase 0 settles the syntax by measurement; phase 1
ships v1 on the Python target; phases 2 to 4 add the agent tooling, the C target and the native
backend.

## Why

lotml exists so that LLMs write correct code more often and reviewers can trust it: Python's
vocabulary where the semantics match, static types, errors as values, value semantics, and a
compiler built as the agent's tool. The study (plans/llm-language-study.md) and its evidence deep
dive (plans/evidence-deep-dive.md) settled what to build and corrected what the first survey got
wrong; what is left is building it in an order where each expensive step is unlocked by a
measurement instead of an opinion. Done when the phase 3 gate passes — the same suite green on the
Python and C targets within 2× C on numeric code — with phase 4 recorded as the open continuation.

## Paths

- `research/` — prototypes the phase 0 harness grows from
- `docs/wiki/pages/` — the evidence each task cites
- `docs/adr/` — the decisions the phases are bound by
- `compiler/` — the Rust compiler, from phase 1
- `harness/` — the evaluation harness, from phase 0

## References

- adr:0001-transpile-to-python-first
- adr:0002-errors-as-values
- adr:0003-value-semantics-with-reference-counting
- adr:0004-python-syntax-where-semantics-match
- adr:0005-significant-indentation
- adr:0006-compiler-written-in-rust
- `docs/wiki/pages/requirements-and-roadmap.md` — requirements R01–R35, phases and gates
- `docs/wiki/pages/evaluation-harness.md` — metrics, the 168-task sample size, gates
- `docs/wiki/pages/source-verification.md` — the 791 checked quotes the tasks cite
- `research/README.md` — how every measurement is reproduced

## Out of scope

- Training or fine-tuning models: phase 2 produces a corpus others can train on; training is a
  separate initiative.
- A package manager, registry or build system beyond compiling and testing a project.
- Self-hosting the compiler in lotml (adr:0006-compiler-written-in-rust).
- Editors beyond the LSP server: the tree-sitter grammar for highlighting is the only editor
  artifact.

## Tasks

- [x] 1.1 (Unit) Decide what `/` does on two `int`s — `f64` as in Python, or an error pointing at `//` — and record it as an ADR before the spec freezes (R34; docs/wiki/pages/type-system.md, docs/wiki/pages/lotml-syntax.md)
- [x] 1.2 (Unit) Review adr:0003 and adr:0005 against the corrected evidence and supersede their context or keep it, as the owner decides (docs/wiki/pages/source-verification.md, docs/wiki/pages/memory-model.md)
- [x] 1.3 (Unit) Write the lotml language reference as idiom examples under 10,000 tokens, variant B, with the parameter conventions, the unit type, `todo()` and the places where semantics differ from Python marked (R12, R24, R28, R33; docs/wiki/pages/training-prior.md, research/pilot/spec-b.md)
  _Depends 1.1_
- [x] 1.4 (TDD) Build the harness task set: HumanEval, MBPP and LiveCodeBench v6 translated to lotml with signatures, docstrings and hidden tests, at least 168 paired tasks per comparison (docs/wiki/pages/evaluation-harness.md, research/experiments/sample_size/results.md)
  _Depends 1.3_
- [x] 1.5 (Unit) Grow the research transpiler into the harness executor: every construct of the reference, value semantics, the step budget and tracebacks at the lotml line (research/experiments/transpiler/, research/experiments/tracebacks/results.md)
  _Depends 1.3_
- [x] 1.6 (TDD) Generate the grammar from the parser's single source in three dialects — llguidance Lark with line-oriented, depth-bounded blocks, GBNF and EBNF — and test each against the whole corpus on every change (R11; research/experiments/grammar/results.md, docs/wiki/pages/constrained-decoding.md)
  _Depends 1.3_
- [x] 1.7 (TDD) Compare variant B against variant A on the task set with at least three model families, closed and open, and report the outcomes that depend on the semantics (adr:0004; research/experiments/transpiler/results.md)
  _Depends 1.4, 1.5_
- [x] 1.8 (TDD) Compare the indented form against a braces form on editing tasks at scale — long files, multi-turn agents, open models, strict and tolerant application — and report slips, rewrites and C-family idioms (adr:0005; research/experiments/editing/results.md, research/experiments/indentation/results.md)
  _Depends 1.4, 1.5_
- [x] 1.9 (Unit) Measure the Claude tokenizer with `count_tokens` on the paired corpus and the generated code, closing the gap in the token measurements (docs/wiki/pages/token-cost.md)
- [x] 1.10 (Unit) Review the phase 0 gate — parse ≥ 95% and no syntactic leakage with the spec, variant and block style confirmed — and record the decisions it settles
  _Depends 1.6, 1.7, 1.8, 1.9_
- [x] 2.1 (Unit) Write the compiler's hand-written, tolerant, recursive-descent parser in Rust on Salsa, reporting positions as UTF-8 offsets and recovering from errors (R10; adr:0006; docs/wiki/pages/transpilation-strategy.md)
  _Depends 1.10_
- [x] 2.2 (TDD) Write the type checker: local bidirectional inference checkable on prefixes, signatures before bodies, records and sum types with exhaustive `match`, `T?` checked before use, monomorphized generics and traits, `todo()` as a value of every type (R02–R04, R06, R33; docs/wiki/pages/type-system.md)
  _Depends 2.1_
- [x] 2.3 (TDD) Implement immutability by default and the parameter conventions — default, `inout` with `&` at the call, `sink` — with closures capturing by copy (R05, R24; docs/wiki/pages/memory-model.md, docs/wiki/pages/python-leakage-pilot.md)
  _Depends 2.2_
- [x] 2.4 (Unit) Implement errors as values: `T ! E`, `?`, `fail`, `??`, and the unit type (R04, R28; adr:0002)
  _Depends 2.2_
- [x] 2.5 (Unit) Emit diagnostics as stable JSON: codes with explanation pages, applicability levels, admissible alternatives, secondary spans, root cause first and bounded by default, SARIF export (R09, R10, R30; docs/wiki/pages/semantic-compiler.md)
  _Depends 2.2_
- [x] 2.6 (Unit) Add the diagnostics for the neighbours' habits — mutating an immutable, truthiness, `raise`, an argument used as a reference, `else if` — each with its fix (R25; docs/wiki/pages/lotml-syntax.md, docs/wiki/pages/editing-robustness.md)
  _Depends 2.3, 2.5_
- [x] 2.7 (Unit) Add `check --fix`, `check --since <rev>` and `check --prefix`, the last never rejecting a completable prefix (R29, R31; docs/wiki/pages/semantic-compiler.md)
  _Depends 2.5_
- [x] 2.8 (TDD) Write the Python backend: an AST carrying lotml positions, inline overflow traps, copies only into `var` and `inout`, `T ! PyError` at the boundary (R13, R26; research/experiments/tracebacks/results.md, research/experiments/overflow/results.md)
  _Depends 2.3, 2.4_
- [x] 2.9 (Unit) Write the canonical formatter, with no options (R08; docs/wiki/pages/editing-robustness.md)
  _Depends 2.1_
- [x] 2.10 (Unit) Run `test` blocks and report them as JSON with observed values (R07; docs/wiki/pages/semantic-compiler.md)
  _Depends 2.8_
- [x] 2.11 (Unit) Add `digest` and `show`: public signatures with no stub bodies, and bodies on demand by symbol (R15; docs/wiki/pages/semantic-compiler.md)
  _Depends 2.2_
- [x] 2.12 (Unit) Ship a prelude that covers the common names without imports (R35; docs/wiki/pages/semantic-compiler.md)
  _Depends 2.2_
- [x] 2.13 (Unit) Review the phase 1 gate on the harness — lotml pass@1 ≥ typed Python, median rounds to green ≤ 2, tokens ≤ typed Python
  _Depends 2.6, 2.7, 2.9, 2.10, 2.11, 2.12_
- [x] 3.1 (Unit) Serve LSP and MCP from the incremental engine, with a warm index and references that carry their surrounding lines (R16; docs/wiki/pages/semantic-compiler.md)
  _Depends 2.13_
- [x] 3.2 (Unit) Offer symbol-addressed edits and atomic refactorings, rename listing textual mentions too (R32; docs/wiki/pages/editing-robustness.md)
  _Depends 3.1_
- [x] 3.3 (Unit) Make lotml callable from Python and generate typed bindings from typeshed's `.pyi` stubs, every call into Python returning `T ! PyError` (R14, R27; docs/wiki/pages/transpilation-strategy.md)
  _Depends 2.13_
- [x] 3.4 (Unit) Build the Python→lotml corpus pipeline: rules for the mechanical part, a frontier model with the compiler for the rest, tests to validate (docs/wiki/pages/training-prior.md, docs/wiki/pages/transpilation-strategy.md)
  _Depends 2.13_
- [x] 3.5 (Unit) Implement colorless concurrency on the Python target with values marked shared when handed to a task, and blocking calls on dedicated threads (R20; docs/wiki/pages/colorless-concurrency.md)
  _Depends 2.13_
- [ ] 3.6 (TDD) Measure in the harness what the literature left open: terse against detailed diagnostics, type masks for open models, and the forgotten-`await` hypothesis (docs/wiki/pages/semantic-compiler.md, docs/wiki/pages/constrained-decoding.md)
  _Depends 3.1_
- [ ] 3.8 (Unit) Add the FFI with C, handing blocking calls to dedicated threads (R17; docs/wiki/pages/colorless-concurrency.md)
  _Depends 3.5_
- [ ] 3.9 (Unit) Generate the tree-sitter grammar for editors from the parser's single source (docs/wiki/pages/requirements-and-roadmap.md)
  _Depends 1.6_
- [ ] 3.7 (Unit) Review the phase 2 gate — incremental adoption from Python working, corpus validated by tests
  _Depends 3.2, 3.3, 3.4, 3.5, 3.6, 3.8, 3.9_
- [ ] 4.1 (TDD) Write the C backend: reference counting with Perceus-style reuse, non-atomic counts, checked arithmetic and `#line` directives (R18, R19; docs/wiki/pages/memory-model.md, docs/wiki/pages/transpilation-strategy.md)
  _Depends 3.7_
- [ ] 4.2 (TDD) Run the same suite on the Python and C targets and require identical results (R18; docs/wiki/pages/transpilation-strategy.md)
  _Depends 4.1_
- [ ] 4.3 (Unit) Benchmark numeric, allocation-heavy and sharing-heavy programs separately against C (docs/wiki/pages/memory-model.md)
  _Depends 4.1_
- [ ] 4.4 (Unit) Review the phase 3 gate — the suite green on both targets, ≤ 2× C on numeric benchmarks
  _Depends 4.2, 4.3_
- [ ] 5.1 (Unit) Add the native backend, Cranelift in debug and LLVM in release (R21; adr:0006)
  _Depends 4.4_
- [ ] 5.2 (TDD) Add effects as capabilities passed as parameters, and measure whether marking them changes what models write (R22; docs/wiki/pages/type-system.md)
  _Depends 4.4_
- [ ] 5.3 (Unit) Add `where` contracts checked in debug (R23; docs/wiki/pages/type-system.md)
  _Depends 4.4_

## Done when

- Every phase gate from 0 to 3 is reviewed and recorded, each with the harness run that decided it.
- The same test suite passes on the Python and C targets, within 2× C on numeric benchmarks.
- `scc validate` exits 0.
