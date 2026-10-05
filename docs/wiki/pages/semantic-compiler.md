# Semantic compiler

The original study calls the compiler "the most important half of the project": a tool an agent
calls in a loop, not just a binary generator. The evidence gathered here confirms the thesis and
corrects three points — how fix confidence is expressed, where errors really are and what the
digest achieves — and adds what the pilot revealed about semantics.

## Where the errors are

- **Types, not syntax.** In LLM-generated TypeScript, "on average 94% of compilation errors
  result from failing type checks"; syntax is about 6%
  ([Mündler et al.](https://arxiv.org/abs/2504.09246), PLDI 2025).
- **Unresolved names lead in Rust.** When translating to Rust
  ([RustRepoTrans](https://arxiv.org/abs/2411.13990)), compile errors are 94.8% of failures, and
  the most frequent codes are unresolved name (E0425), missing method (E0599), unresolved import
  (E0432) and missing trait (E0277); type mismatch (E0308) is far behind.
- **Ownership is a minority.** In 86,726 errors from seven models on CodeNet, ownership and
  lifetimes were 16.7% ([Nogueira, Vieira and Campos](https://arxiv.org/abs/2608.00661), ISSRE
  2026).
- **In lotml, the pilot found semantic errors the parser accepts** — reassigned immutable
  locals, lists mutated without `var`, arguments treated as references
  ([[python-leakage-pilot]]).

Consequence: the priority is name and type diagnostics with suggestions ("did you mean", a
suggested import) and mutability diagnostics with the fix ready. The grammar solves the smallest
part of the problem.

## Diagnostics: follow rustc's model

The original study's JSON format points the right way. The precedents say how to finish it:

- **rustc** ([doc](https://doc.rust-lang.org/rustc/json.html)): a code with an explanation, a
  level, spans with byte and line/column offsets, `children`, `suggested_replacement` and the
  policy "new fields may be added; enumerated fields may add new values".
- **Discrete applicability instead of numeric confidence.** rustc classifies each suggestion as
  `MachineApplicable` (apply automatically), `MaybeIncorrect`, `HasPlaceholders` or
  `Unspecified`, and `cargo fix` only applies the first by default. The original study's
  `"confidence": 0.95` should become that enum: a number invites choosing a threshold, a level
  says what to do.
- **Stable codes, never reused**, each with an explanation page (`rustc --explain`); a retired
  code keeps its page, marked as no longer emitted.
- **Anti-models:** `tsc` has no JSON diagnostics output; GCC removed the `json` format in GCC 16
  in favor of SARIF. Recommendation: a stable JSON format of our own, with SARIF export.

## The repair loop saturates in two or three rounds

- "Successful debugging processes mostly end within 3 turns"
  ([Self-Debugging](https://arxiv.org/abs/2304.05128), ICLR 2024), and richer feedback yielded
  more (MBPP with Codex: 61.4 baseline, 68.2 with simple feedback, 70.8 with a trace).
- "Most models lose 60–80% of their debugging capability within just 2–3 attempts"
  ([Debugging Decay Index](https://arxiv.org/abs/2506.18403), 2025); starting over recovers it.
- Once cost is counted, the gain from self-repair is "often modest"
  ([Olausson et al.](https://arxiv.org/abs/2306.09896), ICLR 2024).

Hence three requirements: **report every error at once** (a tolerant parser), **root cause first
with cascades suppressed**, and **apply `MachineApplicable` fixes without spending a model round**
(`check --fix`).

## The fix format matters as much as the diagnostic

- In [RustAssistant](https://arxiv.org/abs/2308.05177) (ICSE 2025), asking for the whole revised
  snippet dropped accuracy to "below 10%"; a changelog format with original and fixed lines
  reached about 74% on the Stack Overflow cases. GPT-4 fixed 92.59% of 270 microbenchmarks, one
  per error code.
- In aider, turning off flexible patching multiplied editing errors by 9.

Fixes come out as structured patches anchored to symbols, not to line numbers.

## Detailed diagnostics help, with caveats

- [Krishnamurthi and Flatt](https://arxiv.org/abs/2606.01522) (2026), 2,400 trials with an
  agent: repair at 24.6–41.2% with tests only, 34.6–47.8% with the minimal type error, 40.7–63.4%
  with the full unification stack; 97.9% of the repairs that passed the type check also passed
  the tests. They propose separate "terse human" and "detailed AI" modes.
- Detailed feedback does not always help (Zheng et al., ICLR 2025,
  [arXiv 2410.08105](https://arxiv.org/abs/2410.08105)), and the ranking of feedback types varies
  by language ([arXiv 2609.00362](https://arxiv.org/abs/2609.00362)).
- **No controlled study compares, for LLMs, rich Rust- or Elm-style diagnostics with terse
  ones.** It is a question for the [[evaluation-harness]].

## Check on every edit

In [SWE-agent](https://arxiv.org/abs/2405.15793) (NeurIPS 2024), removing lint-on-edit dropped
the result from 18.0% to 15.0% on SWE-bench Lite. 51.7% of trajectories had at least one failed
edit, and the chance of success fell from 90.5% to 57.2% after the first. The linter reverted the
bad edit and showed the error type, the attempted edit and the original text; removing any of the
three parts made things worse. That is why the incremental check in under 100 ms, the original
study's target, is not a luxury: it is what makes checking every edit possible.

## Digest: an index, not a substitute

- **Localization:** in [Agentless](https://arxiv.org/abs/2407.01489), a skeleton of signatures
  localized 58.33% of problems against 53.67% with whole files, at US$0.02 against US$0.15.
- **Types in context:** in [Blinn et al.](https://arxiv.org/abs/2409.00921) (OOPSLA 2024), type
  definitions were "absolutely necessary" and function headers multiplied the result by about 3.
- **Generation:** in [RepoExec](https://arxiv.org/abs/2406.11927) (NAACL 2025 Findings), full
  context won; signature-only beat signature with docstring.

The digest helps find things and makes context cheaper, but whoever is about to change a function
still needs the bodies of its dependencies. Design: the digest is the project's index and bodies
come on demand, by symbol.

## LSP and MCP

- **It became standard:** gopls v0.20 with built-in MCP, `dart mcp-server`, IntelliJ 2025.2,
  Xcode 26.3 (`xcrun mcpbridge`), and Claude Code returns LSP diagnostics after every edit.
- **The gain is unproven.** A preliminary study
  ([arXiv 2608.13568](https://arxiv.org/abs/2608.13568), 2026) found that LSP often *increases*
  token spend (+6% to +118% on symbol localization), and a location-only rename passed 2 of 6
  tasks because it missed mentions in comments and strings.

Design: semantic and textual queries, and atomic refactorings in the compiler (rename also lists
the textual mentions).

## Resulting design

| command | what it returns | why |
| --- | --- | --- |
| `check --json` | every diagnostic, root cause first, with applicability | one round fixes several errors |
| `check --fix` | applies the `MachineApplicable` fixes and reports | trivial fixes without spending a model round |
| `digest <module>` | public signatures, types and documentation | the project's index in context |
| `show <symbol>` | the body of a symbol and of its dependencies | generation needs the bodies |
| `explain <code>` | the error code's page | stable, documented diagnostics |
| `test --json` | results of the `test` blocks | behavior checks |
| `rename`, `refs` | atomic refactoring, semantic and textual mentions | editing without line numbers |
| `fmt` | the canonical form | one way to write it |

Diagnostics aimed at Python habits get codes of their own: mutating an immutable with `var`
suggested, `raise` with `fail` suggested, truthiness with the explicit comparison suggested, an
argument treated as a reference with the `inout` convention suggested ([[memory-model]]). The
grammar published for constrained generation is in [[constrained-decoding]].
