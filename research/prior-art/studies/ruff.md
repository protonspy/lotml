# ruff (parser, AST, salsa db, ty) — Astral's Python linter/formatter and the `ty` type checker, in Rust

Repo `research/prior-art/repos/ruff` (sparse: `ruff_python_parser`, `ruff_python_ast`, `ruff_text_size`,
`ruff_source_file`, `ruff_python_trivia`, `ruff_python_semantic`, `ruff_db`,
`ty_python_semantic/src`, `ty_ide/src`) · MIT (`LICENSE`, "Copyright (c) 2022 Charles Marsh") ·
last commit 2026-10-07 · crates are version 0.0.16, "a component of Ruff 0.16.10", Rust 1.97,
edition 2024 (`Cargo.toml:7-8`). Rust lines in the checkout: `ty_python_semantic` 220,351 ·
`ty_ide` 80,883 · `ruff_python_ast` 28,679 (of which `src/generated.rs` is 338 KB, generated from
`ast.toml` by `generate.py`) · `ruff_python_parser` 21,692 · `ruff_db` 17,283 ·
`ruff_python_semantic` 9,910 · `ruff_python_trivia` 2,755 · `ruff_source_file` 2,165 ·
`ruff_text_size` 1,255. Not in the checkout but referenced: `ty_python_core` (semantic index,
use-def map, place table), `ty_test`/mdtest corpus, `fuzz/` (`Cargo.toml:189`). Maturity:
production parser (the one ruff lints and formats with) and a fast-moving type checker (ty); every
crate warns "The Rust API exposed here is unstable and will have frequent breaking changes"
(`crates/ruff_python_parser/README.md`, "Versioning").

Relevance to LotML: **high** for the frontend and incrementality. ty runs on **salsa 0.28.5, the
exact version LotML pins** (`Cargo.toml:190-195` vs `compiler/Cargo.toml:12`), and its parser is a
hand-written, error-resilient recursive-descent parser for indentation-sensitive Python — the same
problem `lotml-syntax` solves.

## Architecture

Pipeline for ty (the part that matters here):

| Stage | Crate / file | Key types / queries |
|---|---|---|
| Files as inputs | `ruff_db/src/files.rs` | `#[salsa::input] File { path, permissions, revision, status, source_text_override }` (`:357-392`), per-root `Durability` (`:96-160`, vendored stubs `NEVER_CHANGE` `:205`) |
| Source text | `ruff_db/src/source.rs` | `#[salsa::tracked] source_text(db, File)` (`:15`), `line_index(db, File)` (`:216-224`) |
| Parse | `ruff_db/src/parsed.rs` → `ruff_python_parser` | `#[salsa::tracked(returns(ref), no_eq, lru=200)] parsed_module(db, PythonFile)` (`:34`), key `#[salsa::interned] PythonFile { file, python_version }` (`ruff_db/src/lib.rs:31-38`) |
| Lexer | `ruff_python_parser/src/lexer.rs` (2,847 lines) | `Lexer` (`:35-69`), `next_token` (`:142`), `re_lex_logical_token` (`:1277`) |
| Parser | `ruff_python_parser/src/parser/{mod,statement,expression,pattern}.rs` | `Parser` (`mod.rs:67`), `parse_list` (`:723`), `parse_comma_separated_list` (`:812`), `RecoveryContextKind` (`:1147`) |
| AST | `ruff_python_ast` (generated from `ast.toml`) | `Parsed<T> { syntax, tokens, errors, unsupported_syntax_errors }` (`parser/src/lib.rs:387-392`), `AtomicNodeIndex` (`ast/src/node_index.rs`) |
| Semantic syntax errors | `ruff_python_parser/src/semantic_errors.rs` (2,668 lines) | `SemanticSyntaxChecker` — errors the grammar cannot see |
| Semantic index | `ty_python_core` (not cloned) | `semantic_index(db, file)`, `place_table`, `use_def_map`, `ScopeId`, `Definition` (imported in `ty_python_semantic/src/place.rs:2385`) |
| Inference | `ty_python_semantic/src/types/infer.rs` | `infer_definition_types` (`:244`), `infer_scope_types_impl` (`:530`), `infer_deferred_types` (`:410`); module doc on the four granularities (`:1-43`) |
| Diagnostics | `ty_python_semantic/src/types.rs` | `check_types(db, file)` (`:196`) — not tracked; loops scopes and concatenates each scope's diagnostics |
| IDE | `ty_ide/src` | `goto_definition` (`goto_definition.rs:15`), `find_goto_target` (`goto.rs:1418`), `hover` (`hover.rs:20`), `rename.rs`, `completion.rs` |

Ruff the linter uses the same parser but its own non-salsa `SemanticModel`
(`ruff_python_semantic/src/model.rs`, arena IDs `BindingId`, `ScopeId`, `NodeId`). There is no
code generation anywhere: no `run`/`build` split, no IR.

## Frontend

**Lexer** (`ruff_python_parser/src/lexer.rs`)

- **On demand, driven by the parser** (`next_token`, `:142-151`), not a pre-pass. That is what
  makes re-lexing possible: the lexer can be checkpointed and rewound (`checkpoint`/`rewind`,
  `:1447-1490`) and moved back to re-emit a token.
- **Indentation** (`lexer/indentation.rs`): each indentation is measured on two axes — `column`
  (tab to the next multiple of `TAB_SIZE = 2`) and `character` count (`:38`, `add_tab`); if the two
  orderings disagree the comparison fails (`try_compare`, `:72-82`), which is how mixed tabs and
  spaces that are ambiguous become an `IndentationError` without a tab-width assumption. Dedents
  are produced one per `next_token` call from a `pending_indentation` (`lexer.rs:168-190`,
  `handle_indentation` `:307-350`). Backslash continuation inside indentation follows CPython
  issue 90249 (`:250-280`). Form feed resets the column (`:283-287`).
- **Two newline kinds**: `Newline` (logical) and `NonLogicalNewline` (inside brackets or on blank
  lines), both kept as tokens; comments are tokens too (`TokenKind::Comment`), so the full token
  vector (`Tokens`, `ruff_python_ast/src/token/tokens.rs:11`) serves the formatter and the IDE.
- **f-strings / t-strings** are lexed into `FStringStart` / `FStringMiddle` / `FStringEnd` with the
  field expressions as ordinary tokens in between; a stack of `InterpolatedStringContext`
  (`lexer/interpolated_string.rs:1-80`) records the bracket nesting at entry and the format-spec
  depth, so PEP 701 (same quotes inside a field, nested fields in a spec) works.
- **Identifiers**: XID_Start/XID_Continue via `unicode-ident` (`lexer.rs:1591-1620`), names
  NFKC-normalised in the parser (`intern_normalized_name`, `parser/mod.rs:457-465`) and interned
  (`NameInterner`, `:41-56`; `Name` is an inline small string, `ast/src/name.rs:46-69`).
- **`?` is not a token** outside IPython mode (`lexer.rs:439-445`); `!` is only `!=` or the f-string
  conversion marker (`:504-510`).

**Parser** (`ruff_python_parser/src/parser/`)

- Recursive descent for statements, Pratt-style precedence climbing for binary expressions
  (`parse_binary_expression_or_higher`, `expression.rs:245-330`, `OperatorPrecedence`).
- **Never fails**: `parse_unchecked` always returns a `Parsed` with an AST plus `errors`
  (`lib.rs:289-291`). A missing expression becomes an `ExprName` with empty id and
  `ExprContext::Invalid` (`expression.rs:466-492`), not a dedicated error node; its range is empty
  at the previous token's end (`missing_node_range`, `mod.rs:367-379`, with a TODO admitting it is
  wrong for left-side holes).
- **Recovery is by list context**. Every repeated construct is parsed by `parse_list` /
  `parse_comma_separated_list` with a `RecoveryContextKind` (22 kinds: `ModuleStatements`,
  `BlockStatements`, `Elif`, `Arguments`, `Parameters`, `DictElements`, … `mod.rs:1147-1219`).
  Contexts are pushed as a bitset (`RecoveryContext`), and on an unexpected token the parser asks
  "is this token an element or terminator of **any enclosing** list?"
  (`is_enclosing_list_element_or_terminator`, `:931-940`): if yes it stops the inner list instead
  of eating the token, otherwise it reports once (`create_error`, `:1493-1530`, per-context
  messages) and skips one token (`:757-770`). An unexpected `Indent` inside a block is counted so
  its matching `Dedent` is skipped too (`:729-743`, `:762-769`).
- **Unclosed brackets** are repaired by **re-lexing**: when recovery leaves a list, the lexer is
  moved back to the last `NonLogicalNewline` and re-emits it as a `Newline`, decrementing the
  bracket nesting (`re_lex_logical_token`, `lexer.rs:1222-1332`, worked example in the doc
  comment: after `if call(foo` the indented `def bar():` lands back inside the `if` block instead
  of leaving the `if` empty and `bar` at module level).
- **Progress guard**: every list loop calls `ParserProgress::assert_progressing`, which panics if
  the current token id did not change since the last iteration (`parser/progress.rs:35-46`) —
  infinite recovery loops are bugs caught in tests, not hangs in an editor.
- **Token sets** are `u128` bitsets built in `const fn` (`token_set.rs:5-35`).
- **Deep nesting**: `stacker::maybe_grow` after 20 levels (`MAX_UNCHECKED_RECURSION_DEPTH`, red
  zone 100 KiB, 1 MiB segments, `mod.rs:61-64`, `with_recursion` `:161-178`); ty runs checker
  threads with a 16 MiB stack (`ruff_db/src/lib.rs:114`, `STACK_SIZE`). No hard depth error.
- **Version-gated syntax** is parsed and reported separately as `UnsupportedSyntaxError`
  (`lib.rs:387-392`), the same "parse the superset, then refuse with a message" idea as LotML's
  Python-habit diagnostics.
- **Speculative parsing** with `checkpoint`/`rewind` where the grammar is ambiguous
  (`statement.rs:2353-2454`, `:2558-2609`; parser checkpoint `mod.rs:942-965`).

**Spans and lines**: `TextSize(u32)`, `TextRange { start, end }`, `Ranged` trait
(`ruff_text_size`, a fork of rust-analyzer's `text-size`) — the same layout as LotML's `Span`
(`lotml-syntax/src/span.rs:3-7`). `LineIndex` (`ruff_source_file/src/line_index.rs:31-70`) stores
line starts found with `memchr2` over `\n` **and** `\r` (lone CR counts, CRLF once) and an
ASCII flag so columns in ASCII files are byte offsets; it is `Arc`-shared and itself a salsa query
(`source.rs:216`).

**Tests**: `// test_ok name` / `// test_err name` comment blocks inside the parser source are
extracted into `resources/inline/{ok,err}` by a test (`tests/generate_inline_tests.rs`, derived
from rust-analyzer and biome; 123 ok, 256 err files); every fixture is parsed, and **two structural
invariants are asserted on every AST** before the insta snapshot: tokens strictly increasing and
inside the source (`validate_tokens`, `tests/fixtures.rs:402-425`), and every node's range inside
its parent, pre-order strictly increasing, and no node boundary inside a token
(`validate_ast`, `:427-570`). 557 resource files total.

## Semantics and types (ty)

- Gradual typing of real Python: `Type<'db>` is a 16-byte `Copy` enum (`types.rs:1982`, size
  asserted `:12538`) whose payloads are salsa-interned (72 `#[salsa::interned]` in the crate).
  Variants include `Dynamic`, `Divergent` (cycle placeholder), `Recursive`, `Never`,
  `FunctionLiteral`, `BoundMethod`… Compare LotML's `Ty` (`lotml-check/src/ty.rs:25-55`): a tree of
  `Box`/`String`/`Vec`, cloned on use.
- **Four inference granularities as salsa queries** (`types/infer.rs:1-43`): scope (checking a
  file), statement (type context for lambdas), definition (a name's type looked up from another
  scope or module — "with the minimum inference necessary", and how import cycles avoid
  scope-level cycles), expression (shared right-hand sides, narrowing tests), plus deferred
  (string/stub annotations).
- **Cycles by fixed-point iteration**: queries declare `cycle_initial` (start every expression at
  `Divergent`) and `cycle_fn` (normalise each iteration) (`infer.rs:234-243`, `:520-529`); 214
  cycle annotations in the crate. Salsa panics if iteration does not converge (`infer.rs:38-43`).
- **Isolation by result**: `infer_scope_types_impl` reads the whole semantic index and AST, and the
  comment states the design: "Using the index here is fine because the code below depends on the
  AST anyway. The isolation of the query is by the return inferred types" (`infer.rs:546-548`) —
  i.e. early cutoff on equal results, not on narrow inputs.
- **Diagnostics are part of query results**, not salsa accumulators: each `ScopeInference`
  carries its diagnostics and `check_types` concatenates them (`types.rs:196-260`); ruff builds
  salsa **without** the `accumulator` and `rayon` default features (`Cargo.toml:190-195`; salsa's
  defaults are `salsa_unstable, rayon, macros, inventory, accumulator`).
- Narrowing, reachability and place loads are explicit modules (`narrow.rs`, `reachability.rs`
  100 KB, `place_load.rs`), and there are property tests of type relations
  (`types/property_tests.rs`).

## IR and passes

None — ruff and ty stop at the AST plus semantic index. Nothing for `lotml-ir`.

## Backend and toolchain

None. Distribution notes only: release profile sets `codegen-units = 1` for the parser, AST and
salsa (`Cargo.toml:330-335`) and dev profile `opt-level = 3` for salsa (`:355-356`) — salsa is slow
unoptimised even in debug builds.

## Runtime

Not applicable (no code generation). The "runtime" concerns that do exist are salsa's:

- `lru=200` on `parsed_module` — ASTs of files not touched recently are evicted and re-parsed on
  demand through an `ArcSwapOption` (`parsed.rs:21-40`, `:118-140`).
- `heap_size=ruff_memory_usage::heap_size` on every query for memory reporting.
- Cancellation: salsa unwinds a query with `salsa::Cancelled` when an input changes under it;
  `ruff_db::panic::catch_unwind` (`panic.rs:145-180`) turns that (and real panics) into a
  `PanicError` the server can report or retry; a separate `CancellationToken` (`cancellation.rs`)
  exists for long non-salsa operations.

## Testing and conformance

- Parser: inline `test_ok`/`test_err` + structural validators + insta snapshots (above).
- Salsa incrementality: `assert_function_query_was_not_run` / `_was_run` inspect salsa's
  `WillExecute` events (`ruff_db/src/testing.rs:6-130`), so a test states "editing X must not
  re-run query Q on Y".
- ty: Markdown tests (mdtest: code blocks with `# error: [rule]` assertions; runner
  `ty_python_semantic/mdtest.py`, corpus not in the checkout); property tests
  (`types/property_tests.rs`).
- IDE: `cursor_test` with a `<CURSOR>` marker in the source (`ty_ide/src/lib.rs:427-562`) and insta
  snapshots (`ty_ide/src/snapshots`).
- Fuzzing exists (`Cargo.toml:189` refers to `fuzz/Cargo.toml`), not checked out.

## Should LotML swap `lotml-syntax` for ruff's parser?

**No. Do not swap and do not fork; borrow four techniques and two small pieces.** Reasons, each
checked against the source:

1. **The grammars differ at the token level.** LotML needs `?` (postfix try, `T?`), `??`, `!` in
   types (`-> int ! ParseErr`), `&x` arguments, and the keywords `fn type impl trait var inout sink
   fail test dyn` (`lotml-syntax/src/lexer.rs:10-102`, `reference/lotml.md`). Ruff's lexer emits
   `?` only in IPython mode (`lexer.rs:439-445`) and has no `fn`/`var`/`fail`. Statement forms
   differ too: `type P(x: f64)`, `type S = A(..) | B`, `impl T for U:`, `test "name":`, bracket
   types `[int]`, `{str: int}`, `(str, int)` in annotation position. Every one of these would be a
   change to ruff's lexer, parser and AST.
2. **The AST is generated and Python-shaped.** `ruff_python_ast` is produced from `ast.toml` by a
   1,000-line `generate.py` into a 338 KB `generated.rs`; LotML's AST is 538 hand-written lines
   carrying LotML nodes (`FnDef.error`, `Item::Test`, records, sums, `Item::Error`;
   `lotml-syntax/src/ast.rs`). Swapping means maintaining a fork of ~50k lines (parser 21.7k + AST
   28.7k) to replace ~3.4k (`lexer.rs` 702 + `parser.rs` 1,785 + `ast.rs` 538 + `strings.rs` 213 +
   `span.rs` 36).
3. **The API is declared unstable** (README "Versioning"), and the one Python implementation that
   adopted it still had to fork it and add ~2,600 lines to rewrite its diagnostics
   (`RustPython.md`: `Cargo.toml:190-194`, `crates/compiler/src/lib.rs:231`).
4. **LotML's parser carries product features ruff's does not**: a certain fix on errors
   (`SyntaxError.fix`, `parser.rs:12-21`; indentation fix E0002 from the lexer `lexer.rs:478-489`),
   other-language habit diagnostics with fixes (`foreign_operator` `parser.rs:269-288`,
   `at_keyword_habit` `:1351`), all keywords soft (`lexer.rs:105-107`, matching the published
   grammar), an interface mode for `.lotmli` (`parse_interface`, `parser.rs:36`), and
   one-report-per-statement cascade suppression (`reported`, `:186-198`). These are the
   agent-facing contract (adr:0009/0010: tolerant parser reporting wrong indentation with its fix).
5. **ADR position**: adr:0006 and adr:0021 keep "the hand-written parser"; taking ruff's parser
   would add a dependency to `docs/stack.md` and need an ADR — for no capability LotML lacks except
   those listed below, which are cheaper to borrow.

**What to borrow** (MIT, attribution in the file where adapted):

- **Re-lex on unclosed bracket** (technique of `lexer.rs:1222-1332` + `parse_list`'s enclosing
  check). Today an unclosed `(`/`[`/`{` in LotML swallows the rest of the file: the lexer ignores
  newlines and indentation while `depth > 0` (`lotml-syntax/src/lexer.rs:405`, `:416`,
  `:608-611`), so no `Newline`/`Indent`/`Dedent` reaches the parser until EOF and `skip_line`
  (`parser.rs:291-319`) has nothing to stop at. Every later `fn` disappears from the outline,
  symbol-addressed edits and diagnostics. LotML's lexer is a pre-pass, so the cheaper LotML form is
  lexer-side: inside brackets, when a new physical line starts at or left of the indentation of
  the line that opened the bracket **and** begins with a statement keyword (`fn`, `type`, `impl`,
  `trait`, `test`, `return`, `if`, `for`, `while`, …), report the unclosed bracket at its opener,
  reset `depth` to 0 and resume normal layout. S effort, highest value of this list.
- **Recovery contexts as `u128` token sets + "enclosing element or terminator"**
  (`token_set.rs`, `mod.rs:723-940`). LotML recovers per statement (`skip_line`); inside argument
  lists, collection displays and parameter lists it has no list-level recovery, so `f(a, , b)` or a
  stray token in a long dict literal costs the whole statement. LotML has ~94 token kinds
  (`lexer.rs:10-102`), so a `u128` set fits.
- **`ParserProgress`** (`progress.rs`, 46 lines): an assertion that every recovery loop consumed a
  token. Cheap insurance for a tolerant parser the harness feeds mutants (`lotml-ide/src/mutate.rs`).
- **AST/token span invariants in tests** (`validate_tokens`, `validate_ast`,
  `tests/fixtures.rs:402-570`): symbol-addressed edits (adr:0009) and `Item::end`
  (`ast.rs:31-44`) depend on spans being nested and ordered; asserting it over every parser test
  input (and every mutant) finds span bugs before an edit lands in the wrong place.
- Small pieces worth adapting: the **two-axis indentation compare** (`indentation.rs:38-82`) if
  LotML ever wants to refuse ambiguous tab/space mixes instead of fixing tab width at 4
  (`lexer.rs:311-312`, `:457`); **inline `test_ok`/`test_err` fixtures**
  (`tests/generate_inline_tests.rs`) to keep parser examples next to the code that handles them.
- Not worth it: Pratt expressions (LotML's precedence chain `parser.rs:1183-1406` is correct and
  bounded), `stacker` (LotML's hard bounds `MAX_DEPTH` 256 / `MAX_CHAIN` 1000, `parser.rs:85-93`,
  protect every later recursive pass too, which stack growth in the parser alone would not), the
  generated AST, NFKC (LotML's Python target builds an AST, not source, so it never meets
  CPython's tokenizer normalisation).

## Reusable for LotML

| Item | Path in repo | What it gives | LotML crate | License verdict | Effort |
|---|---|---|---|---|---|
| Unclosed-bracket re-lex | `ruff_python_parser/src/lexer.rs:1222-1332`; callers `parser/mod.rs:753`, `:874` | Rest of file survives a missing `)` | `lotml-syntax` lexer/parser | adapt the technique (MIT) — LotML's pre-pass lexer needs its own form, see above | S |
| List recovery contexts | `parser/mod.rs:723-940`, `:1147-1530`; `token_set.rs` | Per-list recovery that defers to enclosing lists | `lotml-syntax/src/parser.rs` (`arguments` `:1462`, `sequence` `:1509`, `brace` `:1606`, params) | adapt (MIT) | M |
| Progress assertion | `parser/progress.rs:1-46` | Panics on a non-advancing recovery loop | `lotml-syntax` | copy (MIT, attribution) | S |
| Span invariant checks | `tests/fixtures.rs:402-570` | Nested/ordered/in-bounds node and token ranges | `lotml-syntax` tests, `lotml-ide` mutant tests | adapt (MIT) | S |
| Inline parser tests | `tests/generate_inline_tests.rs` | Examples in parser comments become fixtures | `lotml-syntax` | adapt (MIT) | S |
| Two-axis indentation | `lexer/indentation.rs:1-130` | Ambiguous tab/space detection | `lotml-syntax/src/lexer.rs:451-491` | adapt only if the policy changes | S |
| `LineIndex` | `ruff_source_file/src/line_index.rs:31-120` | CR/LF/CRLF-consistent line starts, ASCII fast path | `lotml-ide/src/lines.rs:12-21` (splits on `\n` only while the lexer also breaks on a lone `\r`, `lexer.rs:414-421`) and `lotml-syntax/src/span.rs:30-36` (`line_column` is O(n) per call) | adapt (MIT) | S |
| Salsa test helpers | `ruff_db/src/testing.rs:6-130` | Assert a query did / did not execute | `lotml-db` tests (today pointer equality, `lotml-db/src/lib.rs:84-91`) | copy (MIT) | S |
| `catch_unwind` for `Cancelled` | `ruff_db/src/panic.rs:145-180` | Survive salsa cancellation in a server | `lotml` LSP/MCP server, once it runs queries on threads | adapt (MIT) | S |
| Whole parser / AST | `ruff_python_parser`, `ruff_python_ast` | A Python parser | — | nothing (see verdict) | — |
| ty inference | `ty_python_semantic` | Gradual Python typing | — (LotML is statically typed with explicit signatures) | idea only | — |

## Ideas and optimizations worth adopting

Ranked by expected impact on LotML's under-100 ms check-every-edit target
(`docs/wiki/pages/requirements-and-roadmap.md:52`; `transpilation-strategy.md:83-86` cites
rust-analyzer's "typing inside a function's body never invalidates global derived data").

1. **Per-item queries with early cutoff in `lotml-db`.** Today one tracked query checks a whole
   file (`checked`, `lotml-db/src/lib.rs:34-39`), and its result stores absolute spans
   (`Checked::locals: Vec<(Span, Span)>`, `lotml-check/src/lib.rs:83-85`), so any keystroke
   re-checks every function and invalidates `index`, `diagnostics` and everything downstream. ty's
   shape (`infer.rs:1-43`, `:233-260`, `:512-560`): a cheap structural query (semantic index) plus
   per-scope / per-definition inference whose **results** are compared for backdating. LotML is
   the easy case: every signature is annotated (`reference/lotml.md`, "Functions and locals"), so
   a function body's check depends only on the signatures of other items. Proposed queries:
   `items(file)` → item list with signatures, spans **relative to the item's start** (or keyed by
   item index, as ty keys by AST node index, `ruff_python_ast/src/node_index.rs:19-50`);
   `check_fn(file, ItemId)` → that body's types and diagnostics; `diagnostics(file)` → concatenate
   and rebase spans (exactly `check_types`, `types.rs:196-260`). Editing one body then re-runs one
   `check_fn`. Touches adr:0006 (salsa was chosen for this property) — implements it, no conflict.
   Cost: the checker's module-level state (`Checked::functions/declared/methods`,
   `lotml-check/src/lib.rs:86-93`) must become an input to per-function checking. M–L.
2. **Make interfaces inputs and their parse a tracked query, with high durability.**
   `interfaces(db, file)` is a plain function that re-parses every imported `.lotmli` text each
   time `checked` runs (`lotml-db/src/lib.rs:19-26`), i.e. on every keystroke in a file that
   imports a bound Python module. In ty each file is a `File` input, parsing is a query, and stubs
   are `Durability::NEVER_CHANGE` (`files.rs:205`; per-root durability `:139-147`). For LotML:
   one input per interface file, `interface(db, InterfaceFile)` tracked, prelude and bound
   interfaces at `Durability::HIGH` so salsa skips revalidating them on user edits. S.
3. **Repair unclosed brackets** (see the verdict section). The agent loop edits files that are
   transiently broken; today one missing `)` blanks the outline and every symbol address after it.
   Supports adr:0009/0010 (symbol-addressed edits, tolerant parser). S.
4. **What-if checks through the database** (`File::source_text_override`, `files.rs:384-392`,
   "running queries after modifying a file's content but before the content is written to disk…
   to verify that the applied fixes didn't introduce any new errors"). LotML's check-on-edit
   (`Workspace::introduced`, `lotml-ide/src/lib.rs:219-227`) re-parses and re-checks the candidate
   text outside salsa (`check_as`). With idea 1, an override input would reuse every unchanged
   item's cached result, making the pre-edit/post-edit diff (semantic-compiler.md, "Check on every
   edit") proportional to the edit. Depends on 1. S after 1.
5. **`no_eq` on `parse`** (`parsed.rs:34`, rationale `:26-30`). `Parsed` derives `Eq`
   (`lotml-syntax/src/parser.rs:23`), so salsa compares the new AST with the old one on every
   re-parse, and since spans shift on almost every edit the comparison almost never backdates. Only
   meaningful in combination with 1 (cut off at the item query instead). S.
6. **Intern types.** ty's `Type` is 16 bytes and `Copy` with interned payloads (`types.rs:1982`,
   `:12538`); LotML's `Ty` clones `String`s and `Box`es (`ty.rs:25-55`) in a checker that unifies
   constantly. Worth it once per-function queries make types cross query boundaries (they must be
   cheap to compare for backdating). M.
7. **Incrementality tests on salsa events** (`ruff_db/src/testing.rs`): assert "editing body of
   `f` does not execute `check_fn(g)`" — the only way to keep idea 1 from silently regressing. S.
8. **Strict identifier characters.** LotML's lexer accepts any byte ≥ 0x80 in a name
   (`lotml-syntax/src/lexer.rs:435`, `:507`), so `a→b` is one name, while rename validation uses
   `char::is_alphabetic` (`lotml-ide/src/edit.rs:437-442`) — two definitions of "name". Ruff uses
   XID_Start/XID_Continue (`lexer.rs:1591-1620`, crate `unicode-ident`). Adopting it (or ASCII-only
   names) gives one rule and a clear "`→` is not part of lotml" error. A new crate is a
   `docs/stack.md` entry (prior-art rule); ASCII-only needs none. S.
- Not recommended: salsa LRU on `parse` (LotML files are small and few; `lru=200` exists for
  whole-project Python checking); salsa cycle recovery (LotML's explicit signatures mean
  inference never crosses items — keep it that way rather than adopt `cycle_fn`); ty's
  four-granularity inference (definition- and expression-level queries exist to avoid inferring
  unannotated Python, which LotML forbids).

## Pitfalls seen

- **Cycles are a permanent cost of inferring across definitions**: 214 `cycle_*` annotations in
  ty, fixed-point iteration with a Salsa panic on non-convergence "considered a bug"
  (`infer.rs:38-43`), a `Divergent` type variant (`types.rs:1986`). LotML avoids all of it by
  requiring annotations on every signature — a design choice worth defending when generics or
  inference grow.
- **Missing-node ranges are imprecise**: an empty range at the previous token's end, wrong for a
  hole at the left of an expression (TODO, `parser/mod.rs:367-377`), and missing expressions are
  encoded as `ExprName { id: "", ctx: Invalid }` that every consumer must remember to skip. LotML's
  explicit `Item::Error`/error nodes (`ast.rs:1-4`, `:28`) are the cleaner representation; keep
  them.
- **A full token vector costs memory** (`Tokens` kept in `Parsed`, `lib.rs:389`) and drove the LRU
  and `ArcSwapOption` reparse machinery (`parsed.rs:111-140`). LotML keeps only comment spans
  (`parser.rs:27`); add tokens only if the IDE needs them.
- **AST equality is useless for salsa** when nodes store absolute offsets (`parsed.rs:24-30`) —
  the same reason LotML's `parse` early cutoff never fires today.
- **Deep recursion**: ruff needs `stacker` in the parser and a 16 MiB stack for checker threads
  because nothing bounds expression depth; LotML's parse-time bounds are the cheaper policy.
- **Unstable internal crates**: 0.0.x versions tied to a ruff release, "frequent breaking
  changes"; even pinning means following ruff's churn — another reason not to depend on them.
- **f-string field parsing**: ruff needed a dedicated lexer mode stack to support PEP 701. LotML
  lexes an f-string as one `Str` token by scanning to the matching quote
  (`lotml-syntax/src/lexer.rs:526-559`) and parses fields from the substring
  (`parser.rs:1654-1690`), so `f"{d["k"]}"` (valid since Python 3.12, and something models trained
  on recent Python write) ends the string at the inner quote, and `\N{…}` escapes are kept
  verbatim (`strings.rs:77-81`) where Python decodes them. Either support them or refuse them
  with a targeted message; today both fail obscurely.
