# Semantic compiler

The original study calls the compiler "the most important half of the project": a tool an agent
calls in a loop, not just a binary generator. The evidence supports building it that way, but
reading the sources in full corrected several numbers the first survey reported, and newer work
changes three design points: how much to report at once, what a diagnostic must contain, and
checking partial programs while the model is still writing. Every number taken from a paper in `research/literature/sources.json` is quoted in
`claims.json` there and checked against the paper ([[source-verification]]).

## Where the errors are

- **Not syntax.** In LLM-generated TypeScript, 94% of compilation errors came from the type checker
  ([Mündler et al.](https://arxiv.org/abs/2504.09246), PLDI 2025) — six open models of 2B–34B
  parameters on HumanEval and MBPP, with every non-syntactic rejection counted, undeclared names
  included. The authors expect stronger models to make fewer typing errors.
- **Missing methods and unresolved names lead in Rust translation.** In
  [RustRepoTrans](https://arxiv.org/abs/2411.13990) (v6), compile errors are 92.3% of failed
  translations; by count E0599 (missing method, 4,079) comes first, then E0425 (unresolved name,
  2,095), E0308 (type mismatch, 1,035) and E0277 (missing trait, 976). Dependency errors — functions
  and variables of the surrounding repository — cause 67.6% of failures, and a failed translation
  carries 7.7 errors on average, up to 193: cascades inflate every count.
- **Type mismatches lead in self-contained Rust.** In Nogueira et al.
  ([arXiv 2608.00661](https://arxiv.org/abs/2608.00661)), Rust compile errors are type mismatches
  (43.4%), missing imports (20.5%), ownership and lifetimes (16.7%) and traits (6.1%). Missing
  imports dominate C++ (56.6%) but not Java (8.1%): a prelude removes a whole error class.
  Errors needing a non-local fix were repaired far less often than local ones.
- **In lotml, the pilot found semantic errors the parser accepts** — reassigned immutable locals,
  lists mutated without `var`, arguments treated as references — and execution found a test that
  passes only under reference semantics ([[python-leakage-pilot]]).
- **Most failures are not compile errors at all.** Across seven models, assertion failures dominate
  first attempts, and they are repaired about 45% of the time against about 77% for name errors
  ([arXiv 2604.10508](https://arxiv.org/abs/2604.10508)): a compiler sees the minority of what goes
  wrong, which is why `test` blocks belong in the loop.

Consequence: name and method diagnostics with suggestions ("did you mean", a suggested import),
type-mismatch diagnostics that show expected and found, a prelude so common names need no import,
and mutability diagnostics with the fix ready.

## Diagnostics: rustc's model, plus alternatives

- **rustc** ([doc](https://doc.rust-lang.org/rustc/json.html)): a code with an explanation, a level,
  spans with byte and line/column offsets, `children`, `suggested_replacement` and the policy "new
  fields may be added". Its secondary spans matter: RustAssistant used rustc's related locations to
  choose the code it showed the model ([arXiv 2308.05177](https://arxiv.org/abs/2308.05177)).
- **Discrete applicability instead of numeric confidence.** `MachineApplicable`, `MaybeIncorrect`,
  `HasPlaceholders`, `Unspecified`; `cargo fix` applies only the first. A number invites choosing a
  threshold; a level says what to do.
- **Admissible alternatives carry the repair gain.** In the one controlled, paired study of
  feedback content ([arXiv 2607.14167](https://arxiv.org/abs/2607.14167); TextWorld plans, two open
  models, not compiler output), adding the failure's location, the observed value and the list of
  admissible alternatives raised success over a raw message by 42–44 points; location and value
  without the alternatives stayed near the raw baseline. The same content as a keyed JSON record or
  as prose scored the same — JSON helps tools, not the model. For a compiler, the alternatives are
  the candidate names, methods, variants or types in scope.
- **Detail is not monotonic.** Krishnamurthi and Flatt
  ([arXiv 2606.01522](https://arxiv.org/abs/2606.01522)) varied type-error messages for one 14B open
  model on ten small programs: the fuller the message the better on average, with striking
  per-error inversions, and with claude-haiku-4.5 every mode succeeded 87.9–99.7% of the time. Zheng
  et al. ([arXiv 2410.08105](https://arxiv.org/abs/2410.08105)) varied runtime feedback, not compiler
  messages, and found small differences. Hence two modes, terse for people and detailed for agents,
  with the alternatives always present.
- **Stable codes, never reused**, each with an explanation page; JSON of our own with SARIF export
  (`tsc` has no JSON output; GCC 16 removed its JSON format in favour of SARIF).

## How much to report at once

- **A long report hurts.** Checking partial Rust programs and stopping at the first unrecoverable
  error gave reports of one or two diagnostics 65% of the time (mean 5.5), against 40% after full
  generation (mean 13.8, sometimes hundreds); after four restarts the focused reports resolved more
  generations ([Generative Compilation](https://arxiv.org/abs/2607.13921)).
- **Group errors with what a fix introduces.** RustAssistant stalled without grouping an error with
  the errors its fix created, and gave up on groups that stopped progressing.

Requirement: root cause first, cascades suppressed, a bounded report by default and every error on
request — not "every error at once" by default, as the first survey concluded.

## The repair loop: two rounds, then new information

- **Most of the gain comes early.** Two repair rounds captured 76–95% of the achievable improvement
  across seven models, the first round giving the largest single gain; self-repair used 11–54% fewer
  tokens than resampling for comparable pass rates ([arXiv 2604.10508](https://arxiv.org/abs/2604.10508)).
  Blinn et al. capped their loop at two rounds for the same reason.
- **More rounds pay only with new information.** With raw diagnostics, calls beyond four solved
  nothing; with structured feedback one model kept improving through six
  ([arXiv 2607.14167](https://arxiv.org/abs/2607.14167)).
- **Older caveats, read precisely:** Self-Debugging's "trace" is written by the model, not taken from
  running code, and its feedback ordering is not monotonic across models
  ([arXiv 2304.05128](https://arxiv.org/abs/2304.05128)); the Debugging Decay Index measures
  normalized per-attempt effectiveness on HumanEval only, and starting over did not help every model
  ([arXiv 2506.18403](https://arxiv.org/abs/2506.18403)); Olausson et al. measured one repair step
  and found feedback quality the bottleneck ([arXiv 2306.09896](https://arxiv.org/abs/2306.09896)).
- **Route by error kind:** for the weakest model, resampling beat repair; name errors are cheap to
  repair, logic errors are not.

Requirements: apply `MachineApplicable` fixes without spending a model round (`check --fix`); give
the agent a budget of two rounds per error group, extended only when the compiler has something new
to say; and route assertion failures to tests and resampling rather than to more diagnostics.

## The fix format matters as much as the diagnostic

- In [RustAssistant](https://arxiv.org/abs/2308.05177), each feature of the changelog output format
  raised accuracy — line numbers, echoing the original lines, and a description of the fix written
  before the code. Echoing the original makes a patch self-verifying: a changelog whose original
  lines do not match the file is rejected. (The paper's "below 10%" and "about 74%" come from
  different models and benchmarks and are not one comparison.)
- aider reports a 9× increase in editing errors on its Exercism benchmark with flexible patching
  disabled ([docs](https://aider.chat/docs/unified-diffs.html)); the editing pilot found no whitespace failure in 72 search-and-replace
  answers ([[editing-robustness]]).

Fixes come out as structured patches anchored to symbols and to the original text, never to line
numbers alone.

## Check on every edit, against the pre-edit state

SWE-agent's lint-on-edit ([arXiv 2405.15793](https://arxiv.org/abs/2405.15793)) reverts an edit that
introduces a lint error and shows the error, the attempted edit and the original text. Removing it
cost 3 points on SWE-bench Lite in a single run, within the 17.33–18.67% spread of six default runs;
looping on failing edits accounts for 23.4% of unresolved instances. The guard's two costs are
lessons for lotml: it rejects legitimate intermediate states, and it assumes the file was clean
before the edit. lotml's check-on-edit reports only the diagnostics an edit introduces, diffed
against the pre-edit state, and says "no errors" explicitly. The under-100 ms incremental check is
what makes this affordable.

## Checking while the model writes

- **It works on closed models.** Generative compilation streams the model's partial output, seals
  each prefix into a complete program and runs the real compiler on it; with seven models on
  repository-level Rust it cut the compile-error rate from 20.7% (feedback after generation) to
  13.1%, flagged errors a median 3 lines from where the compiler would, and lowered total overhead
  by stopping files that could never compile ([arXiv 2607.13921](https://arxiv.org/abs/2607.13921)).
  It needs no token masks, so it applies to Claude and Gemini, which accept no grammar.
- **Completeness first.** A checker that feeds back diagnostics must never reject a prefix that can
  still be completed; errors that depend on later code — items declared further down, incomplete
  impls — are held until the end of the file. Late detection was dominated by exactly those
  references: unresolved names and imports, missing methods.
- **The language decides how much a prefix check can see** — see [[type-system]]: signatures fixed
  before bodies, definition before use, and a value of every type for an unfinished hole.

Requirement: a `check --prefix` mode that takes a partial file and answers "still completable",
"error here", or "cannot tell yet", in the same incremental engine as `check`.

## Digest: an index, not a substitute

- **Localization:** in [Agentless](https://arxiv.org/abs/2407.01489), a skeleton — signatures plus
  class fields and comments — kept the true locations in 58.33% of cases against 53.67% with whole
  files, at US$0.02 against US$0.15 for that step; without a location clue in the issue, agents with
  search tools did better, so a digest does not replace search.
- **Types in context:** in [Blinn et al.](https://arxiv.org/abs/2409.00921), type definitions mattered
  most in the low-resource language (Hazel, with GPT-4); headers multiplied results by about 3 there
  only together with types, and by 1.5 in TypeScript. Compiler feedback and static context were
  complements: error rounds alone barely helped code built on hallucinated types.
- **Digest format:** bodiless headers written with an ellipsis made models emit `...`; omit the body
  entirely. In [RepoExec](https://arxiv.org/abs/2406.11927), signature-only beat signature with
  docstring only under one prompt format, because bodiless stubs looked like the function to
  complete and models emitted empty functions — a format artifact, not a verdict on docstrings.

Design: the digest is the project's index, bodies come on demand by symbol, and stubs never look
like code to complete.

## LSP and MCP

- **It became standard:** gopls v0.20 with built-in MCP, `dart mcp-server`, IntelliJ 2025.2, Xcode
  26.3 (`xcrun mcpbridge`), and Claude Code returns LSP diagnostics after every edit.
- **The gain depends on the model and the result format** ([arXiv 2608.13568](https://arxiv.org/abs/2608.13568),
  preliminary): LSP raised token spend by 6–118% for Opus and Sonnet but saved Haiku 26%;
  location-only references were the costly part — adding two lines of source around each raised
  rename pass@1 from 0.67 to 0.83 and cut tokens by 19%. A cold index silently returned incomplete
  references, and agents seldom used semantic tools unless the task was reference-shaped.

Design: semantic and textual queries that return the referenced lines, atomic refactorings in the
compiler (rename also lists textual mentions), and an index that is warm before the agent asks.

## Resulting design

| command | what it returns | why |
| --- | --- | --- |
| `check --json` | diagnostics root cause first, bounded by default, with applicability and alternatives | small, focused reports repair better |
| `check --prefix` | completable, error here, or not yet, for a partial file | partial-program checking works on closed models |
| `check --since <rev>` | only the diagnostics an edit introduced | check-on-edit without blocking pre-existing errors |
| `check --fix` | applies the `MachineApplicable` fixes and reports | trivial fixes without spending a model round |
| `digest <module>` | public signatures, types and documentation, no stub bodies | the project's index in context |
| `show <symbol>` | the body of a symbol and of its dependencies | generation needs the bodies |
| `explain <code>` | the error code's page | stable, documented diagnostics |
| `test --json` | results of the `test` blocks, with observed values | most failures are logic, not compile errors |
| `rename`, `refs` | atomic refactoring; references with surrounding lines | location-only results cost tokens |
| `fmt` | the canonical form | one way to write it |

Diagnostics aimed at the neighbours' habits get codes of their own: Python's — mutating an
immutable with `var` suggested, `raise` with `fail` suggested, truthiness with the explicit
comparison suggested, an argument treated as a reference with `inout` suggested ([[memory-model]]) —
and the C family's, such as `else if` with `elif` suggested ([[editing-robustness]]). The grammar
published for constrained generation is in [[constrained-decoding]].

## What phase 1 built

The `lotml` command in `compiler/` (adr:0006-compiler-written-in-rust) has every row of the table
above except `rename` and `refs`, over Salsa queries:

- **`check`** — text, versioned JSON or SARIF; root causes first across files (syntax, then names
  and declarations, then types, then mutability) and five shown unless `--all`; every code has an
  `explain` page. On the 645 variant B answers that passed their hidden tests in phase 0, it
  reports 15, each a real departure from the reference (`int(str)`, `int ** f64`, `fail` in a
  function that cannot fail, an optional used unchecked).
- **`check --fix`** applies the machine-applicable fixes; the neighbours' habits — `var`,
  truthiness, `raise`, `&`, `else if`, `def`, `true`/`false`/`null`, `let`, `&&`/`||`/`!`, `x++`,
  the `typing` spellings — each come with one, and applying them leaves a program that checks.
- **`check --since <rev>`** keeps the diagnostics an edit introduced, matched against the file at
  that revision by code, message and line text, and says "no errors introduced" when there are none.
- **`check --prefix`** answers completable, error or unknown. An error stands only if it is there
  without the unfinished last line too, so a signature still being typed does not condemn its
  callers; on the 706 stored programs that check clean, none of 268,030 prefixes was an error.
- **`digest`** prints the types in full and every signature with its documentation as comments
  above it — never a bodiless stub; **`show`** prints a symbol as written and what it uses.
- **`test --json`** reports each `test` block: pass, fail with the value each side of the
  comparison had, an error passed on by `?`, or a panic with its line.
- **`fmt`** is the canonical form; on 1,509 stored programs it changes no tree and loses no comment.

## What phase 2 adds

The same engine serves an editor and an agent (`compiler/crates/lotml-ide`), with no protocol
library — JSON-RPC over `serde_json`:

- **What each name refers to.** The checker records, for every local, parameter and binding,
  the name that declared it — a local declared in each branch of an `if` is one local — and the
  rest is resolved by name: functions, types, traits and variants by declaration, fields and
  methods through the checked type of the value they are read from (`c.get()` is `Counter.get`
  because `c` is a `Counter`), keyword arguments to the parameter or field they name. Each file
  is its own module, so a name refers to a declaration in the same file.
- **`lotml lsp`** loads and checks every `.lotml` file under the workspace's folders when the
  client connects, so the first question meets a warm index, and publishes their diagnostics —
  notes and alternatives in the message, labels as related locations. It answers definitions,
  references, hover (a local's type, a declaration's signature and documentation), outlines,
  workspace symbols, formatting, and the diagnostics' fixes as quick fixes, the
  machine-applicable ones preferred. Columns are UTF-16 units unless the client accepts UTF-8.
- **`lotml mcp`** offers `check`, `digest`, `show`, `references`, `definition`, `hover`,
  `explain` and `test` as tools. A reference comes with the two lines around it and its own
  marked, the inline context that recovered most of the location-only results' loss
  ([arXiv 2608.13568](https://arxiv.org/abs/2608.13568)). Files changed on the disk are read
  again before each call and only they are checked again. It speaks both eras of the protocol:
  the `initialize` handshake and the stateless requests of the 2026-07-28 revision.
