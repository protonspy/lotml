# interpreter-rs — a Monkey-style language with a tree-walking evaluator and a bytecode compiler + VM as two implementations over one shared parser

`research/prior-art/repos/interpreter-rs` · **no license file** (ideas only, never code) · last commit
2023-02-08 · Rust 5,154 lines, of which ~2,040 are unit tests; by crate (non-test):
`lexer` 607, `parser` 1,128, `object` 297, `evaluator` 369, `compiler` 356, `bytecode` 24,
`vm` 158, `repl` 108, `repl2` 56. Maturity: a learning project following Thorsten Ball's books
("Writing an Interpreter/Compiler in Go", `README.md:5`); the evaluator is the complete side, the
compiler is a partial port (`README.md:13`), and the workspace does not build from a clean clone
(Pitfall 1). Small and only tangentially relevant: the study is kept proportional.

All paths below are relative to `research/prior-art/repos/interpreter-rs/` unless they start with
`compiler/` or `docs/adr/` (LotML).

## Architecture

```
source ─ lexer::Lexer ─ PeekLex ─ parser::parse ─ Vec<Statement> ─┬─ evaluator::eval ─ EvalObj        (repl)
                                                                 └─ compiler::Compiler ─ Vec<OpCode> ─ vm::VM   (repl2)
```

- `lexer` (`crates/lexer/src/lexer.rs:35` `next_token`, `crates/lexer/src/lib.rs:12-16` `PeekLex`
  with two-token lookahead), `parser` (`crates/parser/src/lib.rs:15` `parse`; AST in
  `crates/parser/src/expr.rs:9-73` and `statement.rs:8-14`) are the shared frontend.
- `object` (`crates/object/src/lib.rs:11-25`) is the shared value type, with one variant per
  backend behind Cargo features: `EvalFunc{parameters, body, env}` under `evaluate`,
  `CompFunc(Vec<OpCode>)` under `compile` (`crates/object/Cargo.toml` `[features]`).
- `evaluator` walks the AST (`crates/evaluator/src/lib.rs:16-63, 65-155`); `compiler` emits a flat
  `Vec<OpCode>` (`crates/compiler/src/lib.rs:30-38`; opcodes `crates/bytecode/src/lib.rs:2-24`);
  `vm` is a stack machine (`crates/vm/src/lib.rs:49-103`).
- Two binaries pick a path: `repl` runs the evaluator (`crates/repl/src/main.rs:26-55`), `repl2`
  compiles and runs the VM (`crates/repl2/src/main.rs:38-56`). Nothing runs both.

**The sharing pattern**: the parser's `Statement`/`Expr` enums are the only contract between the
two implementations. There is no resolution, checking or lowering layer after the parser; each
backend resolves names its own way — an `Arc<Environment>` chain looked up at run time
(`crates/object/src/env.rs:62-71`, `evaluator/src/lib.rs:114-121`) versus a compile-time
`SymbolTable` (`crates/compiler/src/symbol_table.rs:30-41`, `compiler/src/lib.rs:97-101`).

## Frontend

- Lexer splits the input into lines (`lexer.rs:15-26`) and scans bytes, casting each to `char`
  (`lexer.rs:40, 61-160`); spans are `(row, col)` pairs (`token.rs:166-171`) combined with `+`
  (`token.rs:181-191`). Unknown characters become `TokenKind::Illegal`; unterminated strings and
  invalid UTF-8 are errors.
- Parser: recursive descent with Pratt-style precedence (`parser/src/lib.rs:197-244`,
  `Precedence` `lexer/src/token.rs:6-23`). Semicolons decide whether an expression statement
  yields its value (`Statement::Expression{terminated}`, `parser/src/expr.rs:94-115`).
- Error handling: errors are accumulated into one `error_stack::Report` with `extend_one`
  (`parser/src/structs.rs:29-36, 43-54`) and the statement loop keeps going after a bad statement
  (`parser/src/lib.rs:35-88`), with `Help`/`Suggestion` attachments (`parser/src/error.rs:36-58`).
  Crude recovery: a failed statement has consumed an arbitrary number of tokens.
- No indentation, no incrementality. Every AST node derives `Clone, Hash, Eq`
  (`expr.rs:8`, `statement.rs:7`) — needed because constants and function bodies are hashed for
  deduplication (`compiler/src/lib.rs:79-111`).

Nothing here goes beyond `lotml-syntax`, which already has a tolerant parser and indentation.

## Semantics and types

Dynamically typed, no checker. Integers, booleans, strings, arrays, closures; `let`, assignment,
`return`, `if`/`else if`, function literals (`README.md:9-13`). Type errors surface at run time in
the evaluator (`EvalError::UnexpectedObject`, `evaluator/src/lib.rs:98-111`) and as `panic!`/
`unimplemented!` in the VM (`vm/src/lib.rs:118, 144-146`). Nothing applies to LotML's static
checker.

## IR and passes

- `Vec<OpCode>` is the only IR, produced directly from the AST, no passes. Constants are
  deduplicated through a `HashMap<Object, usize>` and later sorted into a vector
  (`compiler/src/structs.rs:5-8, 28-40`). Jumps are back-patched (`compiler/src/lib.rs:150-202`).
- The evaluator has no IR at all.

## Backend and toolchain

A bytecode VM, not native code: `Const`, `Get/Set/CreateGlobal`, arithmetic, comparisons,
`Jump`/`JumpNotTruthy`, `Call` (`bytecode/src/lib.rs:2-24`). `Call` recursively re-enters `run`
with the callee's bytecode on the same stack (`vm/src/lib.rs:75-88`). No linker, no toolchain, no
JIT. Nothing applies to `lotml-llvm`.

## Runtime

- Values: `Object` enum, cloned freely (`object/src/lib.rs:10-25`); `EvalObj{is_return, obj}`
  wraps it so `return` can unwind through the evaluator (`object/src/lib.rs:128-158`).
- Environments: `Mutex<HashMap<String, EvalObj>>` behind `Arc`, chained via `outer`
  (`object/src/env.rs:29-33`); blocks do not open a scope (`evaluator/src/lib.rs:122-128` evaluates
  a scope in the enclosing `env`).
- VM state: fixed 100-slot stack, globals vector (`vm/src/lib.rs:39-47`).
- No memory management beyond Rust ownership and `Arc`; no interop.

## Testing and conformance

- Per-crate golden tests with `expect-test`'s `expect_file!` (e.g. `vm/src/tests.rs:19-33`): each
  test asserts the value and snapshots `format!("{vm:#?}")`, the VM's internal debug dump
  (constants, `sp`, top of stack; `vm/src/lib.rs:17-25`), into
  `crates/*/tests/expect_test_results/*.txt`.
- No differential test between the evaluator and the VM, though they run the same programs; the
  two suites are separate files with separately chosen inputs (`evaluator/src/tests.rs`,
  `vm/src/tests.rs`).
- A criterion benchmark of the lexer (`crates/lexer/benches/lex_large_file.rs`) — see Pitfall 5.

## Reusable for LotML

| Item | Path in repo | What it gives | LotML crate | License verdict | Effort |
|---|---|---|---|---|---|
| Golden files that rewrite themselves | `crates/vm/src/tests.rs:19-33` (uses the `expect-test` crate) | `UPDATE_EXPECT=1` regenerates snapshots; diffs show in review | `lotml-ir` tests comparing IR text (`compiler/crates/lotml-ir/src/text.rs:1-3`) | idea only from this repo (no license); `expect-test` itself is MIT/Apache, but adding it is a new dependency: `docs/stack.md` entry first | S |
| Shared parser, separate backends | `crates/parser`, `crates/evaluator`, `crates/compiler` | — | — | nothing: adr:0020 already chose one IR between checker and every backend | — |

Nothing else: no code may be copied, and no idea here improves on what LotML has.

## Ideas and optimizations worth adopting

1. **Confirmation of adr:0020, with the cost made visible.** Sharing only the parser left every
   rule after parsing to be written twice, and the second copy stalled: the compiler ignores call
   arguments (`compiler/src/lib.rs:113-121`), leaves arrays, indexing, scopes, method calls, `&&`,
   `||` and bit operators as `todo!()` (`compiler/src/lib.rs:103, 122, 129-130, 245-248`), and
   compiles `return` as `Jump(9999)` (`compiler/src/lib.rs:57-62`). The evaluator and compiler even
   find unknown names at different times (run time vs compile time). LotML's
   `lotml-ir` exists to prevent exactly this; no action, but it is a second data point (with plix)
   for `docs/wiki/pages/transpilation-strategy.md`. No ADR conflict.
2. **Self-updating snapshots for IR text and diagnostics** (low impact). If the number of IR-text
   expectations in `compiler/crates/lotml-ir/tests/` grows, an `expect-test`-style update mode keeps
   them cheap to maintain. Snapshot printed IR or diagnostics, never a `Debug` dump of internal
   state (Pitfall 4). Needs a `docs/stack.md` entry; no ADR conflict.

## Pitfalls seen

1. **Workspace depends on an absolute local path**: `error-stack = {path = "/Users/jaredmoulton/Developer/hash/main/packages/libs/error-stack/"}`
   (`Cargo.toml` `[workspace.dependencies]`), so no clean clone builds.
2. **Feature-gated shared value type.** `Object`'s backend-specific variants live behind
   `evaluate`/`compile` features, but `EvalObj::inner` matches both variants without `#[cfg]`
   (`object/src/lib.rs:147-156`), so `object` compiles only when Cargo unifies both features —
   building `evaluator` alone should fail. Sharing a type by toggling its variants couples the
   backends invisibly; LotML's IR types carry no per-backend variants, which should stay so.
3. **Errors swallowed or turned into panics.** `Compiler::compile` returns `Vec<OpCode>` and drops
   every `Result` it gets (`compiler/src/lib.rs:30-38, 45`); `PeekLex::next` maps a lexer error to
   end of input (`lexer/src/lib.rs:20-31`) and `Lexer::next` prints and returns `None`
   (`lexer/src/lexer.rs:266-276`), while `PeekLex::peek` unwraps it and panics
   (`lexer/src/lib.rs:47-56`); VM integer overflow and division by zero panic
   (`vm/src/lib.rs:127-131`).
4. **Snapshots of internal state.** VM tests snapshot `{vm:#?}` (`vm/src/tests.rs:27`), so any
   change to constant numbering or stack layout rewrites goldens unrelated to behavior. The
   evaluator crate also carries 38 orphaned copies of parser and lexer goldens (36 + 2)
   (`crates/evaluator/tests/expect_test_results/{parser,lexer}/`) referenced by no test and already
   differing from the parser's own (`diff -rq` against `crates/parser/tests/expect_test_results`).
5. **A benchmark that measures one token.** `for _ in lexer.next() {}` iterates the single `Option`
   returned by one `next()` call (`crates/lexer/benches/lex_large_file.rs:6`), so the "large file"
   benchmark lexes one token of a 62 KB input.
6. **VM semantic bugs from the second implementation.** `GetGlobal` moves the value out of the
   global slot with `mem::replace` (`vm/src/lib.rs:66-69`), so reading a global twice yields
   `Empty`; nested function bodies are compiled with a fresh `SymbolTable` and cannot see globals
   (`compiler/src/lib.rs:104-112` → `compile` at `:30-38`). The evaluator has neither bug — the
   divergence a differential test would have caught.
