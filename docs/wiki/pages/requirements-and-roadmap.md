# Requirements and roadmap

The original study's consolidated requirements, revised item by item, plus those the research and
measurements added, and the roadmap with its measured gates. Requirements marked v1 form the MVP.
This page is the basis for each feature's spec; the decisions that are expensive to reverse are in
the accepted ADRs under `docs/adr/`.

## Functional and tooling requirements

| ID | requirement | phase | status |
| --- | --- | --- | --- |
| R01 | indentation-based syntax with Python's vocabulary | v1 | kept, conditional on the [[evaluation-harness]]'s editing test |
| R02 | mandatory types in signatures, fields and exports; local inference | v1 | **adjusted:** local, bidirectional inference, checkable on prefixes ([[type-system]]) |
| R03 | records, sum types and exhaustive `match` | v1 | **adjusted:** variants accept positional fields |
| R04 | `T?` as the only form of absence; `T ! E` with `?` and `fail` | v1 | **adjusted:** `??` and `is None`, from variant B (adr:0004-python-syntax-where-semantics-match) |
| R05 | immutable by default; `var` for mutable | v1 | kept |
| R06 | monomorphized generics and traits | v1 | kept |
| R07 | `test` blocks next to the code | v1 | kept; equality with an error written `== Err(E)` |
| R08 | canonical formatter with no options | v1 | kept |
| R09 | JSON diagnostics with stable codes and applicable fixes | v1 | **adjusted:** applicability levels (`MachineApplicable`…) instead of numeric confidence; an explanation page per code ([[semantic-compiler]]) |
| R10 | tolerant parser, every error at once | v1 | kept; root cause first, cascades suppressed |
| R11 | published formal grammar | v1 | **adjusted:** generated from the same source as the parser, tested against the corpus, in three dialects — llguidance-compatible Lark, GBNF and EBNF ([[constrained-decoding]]) |
| R12 | complete specification under 10,000 tokens | v1 | feasible: the pilot's core spec is about 1,830 |
| R13 | transpilation to Python with errors pointing at the source | v1 | **adjusted:** Python AST with the original positions; a minimal version already in phase 0 ([[transpilation-strategy]]) |
| R14 | language functions callable from Python | v1 | kept |
| R15 | `digest` command | v2 | **recommended for v1**, with `show <symbol>` for the bodies: cheap once the type checker exists |
| R16 | LSP and MCP server | v2 | kept; atomic refactorings and textual queries alongside semantic ones |
| R17 | FFI with C | v2 | kept; blocking calls handed to dedicated threads |
| R18 | transpilation to C with the same semantics as the Python target | v2 | kept; checked arithmetic instead of undefined overflow, `#line` |
| R19 | value semantics with reference counting and elision | v2 | **adjusted:** Perceus-style reuse, borrow inference, non-atomic counting per task ([[memory-model]]) |
| R20 | colorless concurrency | v2 | kept ([[colorless-concurrency]]) |
| R21 | native LLVM and Cranelift backend | v3 | kept |
| R22 | effect system (`io`) | v3 | **adjusted:** start with a capability passed as a parameter; no evidence of benefit for LLMs |
| R23 | `where` contracts checked in debug | v3 | kept |
| R24 | parameter conventions (default, `inout`, `sink`) with a visible marker at the call | v1 | **new** — the pilot showed the model inventing reference semantics |
| R25 | diagnostics aimed at Python habits (mutating an immutable, truthiness, `raise`, argument by reference) with applicable fixes | v1 | **new** |
| R26 | a single overflow semantics (trap) in every build and on every target, with explicit modular arithmetic | v1 | **new** |
| R27 | typed bindings generated from `.pyi` stubs; every call into Python returns `T ! PyError` | v2 | **new** |
| R28 | a defined unit type for functions that fail without returning a value | v1 | **new** |

## Non-functional requirements

| requirement | status |
| --- | --- |
| incremental check of one file in under 100 ms | kept; it is what makes checking every edit possible |
| debug build of 10,000 lines in under 2 s | kept |
| programs ≥ 20% smaller in tokens than the equivalent Python, on 3+ tokenizers | **replaced** by "no larger than the equivalent typed Python, on 3+ tokenizers": variant A measures 9–11% fewer and SimPy 9–14% ([[token-cost]]) |
| pass@1 equal to or above typed Python's on the same benchmark | kept; it becomes the primary metric |
| release performance within 2× C on numeric benchmarks | kept; allocation-heavy benchmarks reported separately |
| parse ≥ 95% and no syntactic leakage for frontier models with the spec | **new** (pilot: 93%, 97% with R03 adjusted) |
| median rounds to green ≤ 2 with structured diagnostics | **new** |

## Roadmap

The original study's phase diagram did not survive its export to `docs/raw/` (it appears as
"embedded content: roadmap · 5 fases, 4 portões"); the phases below follow its text, with this
research's adjustments. Each gate's criteria are in [[evaluation-harness]].

| phase | deliverable | gate to leave |
| --- | --- | --- |
| 0 | harness with external and editing tasks, spec, grammar, checkers, minimal transpiler to Python | parse and leakage; variant and block style confirmed by data |
| 1 | v1 on the Python target: types, `match`, errors, `var` and `inout`, tests, `fmt`, `check --json`/`--fix`, digest | pass@1 ≥ typed Python; rounds ≤ 2; tokens ≤ typed Python |
| 2 | full semantic compiler (LSP, MCP, refactorings), stub-based bindings, Python→lotml corpus, concurrency | incremental adoption working; corpus validated |
| 3 | C target with reference counting and reuse, parity tests across targets | same suite on both targets; ≤ 2× C |
| 4 | native backend (Cranelift in debug, LLVM in release), effects, contracts | — |

**The original study's technical choices:** a compiler in Rust, a hand-written recursive-descent
parser, a query-based incremental architecture (Salsa), a tree-sitter grammar for editors. One
data point to weigh: Roc rewrote its compiler from Rust to Zig to cut the compiler's own
incremental rebuild time (3.4 s to about 35 ms), at a cost of 487 days — the implementation
language is expensive to change, and the decision is recorded in
adr:0006-compiler-written-in-rust.
