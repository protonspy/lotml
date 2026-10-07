# mamba — Python-like language with strict static typing, transpiled to Python source, plus a small Cranelift native backend

Repo `JSAbrahams/mamba` · MIT (`LICENSE`, referenced by `license-file` in `Cargo.toml`) · last
commit 2026-10-03 · v0.3.6 · Rust, one crate: `src/parse` 8.8k lines, `src/check` 11.0k,
`src/backend/python` 4.6k, `src/backend/cranelift` 1.5k, `src/common` 0.4k; `tests` 0.9k +
`tests_util` 0.4k + 436 fixture files; Python stubs for builtins 320 lines · Maturity: a
long-running personal language, actively developed, small; the parser stops at the first error
and the native backend handles `Int`/`Float`/`Bool` only.

Paths below are relative to `research/prior-art/repos/mamba/` unless they start with `compiler/` (LotML).

## Architecture

- **Shared front half**: `check_sources` (`src/lib.rs:148-196`) parses every file
  (`str::parse::<AST>`), builds one `Context` from all ASTs plus the Python stub resources,
  checks each file and returns typed trees (`ASTTy`). It is "shared by every backend"
  (`src/lib.rs:148-149`).
- **Python backend** (the default): `ASTTy` → `PythonCore` (`src/backend/python/ast/node.rs`) by
  the converters in `src/backend/python/convert/*`, threading a `State` and an `Imports` set;
  `PythonCore` is printed by `Display`; one `.py` per `.mamba`, mirroring the source tree
  (`src/backend/python/mod.rs:28-84`).
- **Cranelift backend** (`--bin`, `--asm`): `ASTTy` lowered straight into a
  `cranelift_object::ObjectModule`, "no intermediate `PythonCore`-style tree"
  (`src/backend/cranelift/README.md:7-8`); linked by the system `cc`
  (`src/backend/cranelift/link.rs:6-23`). `Backend` enum: `src/backend/mod.rs`.
- **Run vs build**: there is no `run` command; the user runs the generated Python. Native output is
  an executable or disassembly. Both backends start from the typed AST; there is no IR between
  the checker and the emitters — the opposite of LotML's ADR 0020.

## Frontend

- Lexer `src/parse/lex/tokenize.rs` → `Vec<Lex>`, one post-pass (`src/parse/lex/pass/docstring.rs`).
- Parser: hand-written recursive descent over a `LexIterator` (`src/parse/iterator.rs`); a
  look-ahead with rollback, `peek_if_skipping`, decides optional trailing keywords such as `else`
  past newlines (`CLAUDE.md:176-181`).
- **Blocks are no longer indented.** The grammar moved to `do … end` ("Block syntax (post
  indent/dedent removal)", `CLAUDE.md:170`); `if`/`then`/`else` branches take one
  expression-or-statement (`CLAUDE.md:172-175`). The design documents still say "Use indentation
  to denote code blocks" (`docs/philosophy/readability.md:33`). No reason for the change is
  written down in the files (the clone is shallow, so the history was not read); it is a data
  point against significant indentation (ADR 0005, ADR 0009) with no evidence attached.
- **Fail-fast**: the first `ParseErr` is returned (`src/parse/mod.rs:31-41`); no recovery, no
  partial tree, no incremental reparsing.
- Every `AST` node carries a `Position` (line/column carets).

## Semantics and types

- Three phases (`src/check/README.md`): build a `Context` of class, function and field signatures
  from every AST and from Python stubs, without checking bodies; generate constraints with an
  expected super-type direction (`src/check/constrain/generate/*`); unify them into a
  `Finished { pos_to_name: HashMap<Position, Name> }` (`src/check/constrain/unify/finished.rs:17-66`).
  The typed tree is the untyped one with each node's type looked up **by its source position**
  (`src/check/ast/mod.rs:33-45`).
- **Names**: a `Name` is a set of `TrueName`s — a union; a `TrueName` is a `NameVariant`
  (nominal `StringName`, `Tuple`, `Function`) plus nullable and mutable flags; mutability is
  display-only and left out of equality and hashing (`CLAUDE.md:143-145`, `src/check/name/`).
- **Python's builtins and stdlib** are `.py` stub files (`src/check/resource/primitive/*.py`,
  `src/check/resource/std/*.py`) parsed **at check time** with `ruff_python_parser`
  (`src/check/context/python.rs:22-33`), as Python 3.14 (`python.rs:19`), "once per `check_all`
  call" (`tests/README.md:74`).
- **Imports are refused** by the checker unless the `imports` feature is on — "the language itself
  has no module system" (`Cargo.toml:16-18`, `src/check/constrain/generate/mod.rs:105-111`).
- **Errors are checked exceptions**: a function lists what it raises; a call must sit in a handler
  for those classes or in a function that declares them (`check_raises_caught`,
  `src/check/constrain/generate/statement.rs:73`, called at `call.rs:121` and `232`); the handler
  syntax is `f(10) ! where err: MyErr => … end` (`CLAUDE.md:183-187`). The Python backend turns
  a handle into `try`/`except` (`src/backend/python/convert/handle.rs:9-81`). LotML's `T ! E`
  (ADR 0002) carries the error as a value instead, but its Python target also propagates `?`
  through an exception internally (`compiler/crates/lotml-py/runtime/lotml_rt.py:104-108`).
- **Mutability** (`mut` on bindings) and **purity** (`pure`) exist; "What is actually enforced is
  narrower than what is written down" (`CLAUDE.md:236`), each gap pinned by an `ignore[...]`
  fixture (`CLAUDE.md:239`).
- Generics are Python `Generic[T]` classes; no monomorphization. Dynamic Python features are not
  in the language; Python is reached only through the stubs.

## IR and passes

There is no IR. `PythonCore` is a Python-shaped tree, and the conversions carry the semantic work:

- `hoist_constructor_dependent_stmts` + `order_by_self_field_deps`
  (`src/backend/python/convert/class.rs:125`, `203`, `260`): statements in a class body run per
  instance, so they move into a generated `__init__`, ordered by the fields they read.
- Destination-passing for expression-valued blocks: `State.must_assign_to` and
  `is_last_must_be_ret` (`src/backend/python/convert/state.rs:21-22`) make the last expression of
  each branch of an `if`/`match`/handle an assignment to a target or a `return`.
- `handle` → `try`/`except` (`convert/handle.rs`).
- Cranelift: one walk, `lower_stmt` / `lower_tail` / `lower_expr` (`src/backend/cranelift/README.md:69-74`).

## Backend and toolchain

- **Python**: text through `Display`; `--annotate` adds type hints to the output.
- **Cranelift 0.114** (`Cargo.toml:23-27`): `Int`, `Bool`, `Float`; arithmetic and comparisons
  picking `iadd`/`fadd` and `icmp`/`fcmp` from the operand's resolved type; `if`; `for … in a ..
  b`; top-level functions; `print` through libc `puts`/`printf` (`src/backend/cranelift/README.md:26-46`). Top-level
  statements become a synthetic `main`. A `Float` cannot be printed because `printf` needs the
  SysV variadic convention (`%al` = vector-register count), which the backend lacks
  (`src/backend/cranelift/README.md:43-46`). `Int` becomes `I64` and wraps: "A program that exceeds that range silently
  disagrees with the same program run through the Python backend" (`src/backend/cranelift/README.md:50-56`).
- Target: host via `cranelift-native`, or a triple via `target-lexicon`. Linking: `cc` only
  (`link.rs:6`) — nothing for Windows, where `cc` is not normally present.
- No JIT, no shared libraries, no runtime library.

## Runtime

None of its own. Generated Python uses CPython's builtins; tuple annotations import from
`typing`. The native backend calls libc directly.

## Testing and conformance

- Table-driven `#[test_case]` tests over `tests/resource/{valid,invalid}/<category>/<name>.mamba`.
  A valid program is transpiled and compared against a reference `.py` **by Python AST**
  (ruff's range-insensitive comparison), after both files pass `python -m py_compile`
  (`tests_util/src/lib.rs:246-320`).
- `tests/execution.rs` runs fixtures through both backends and asserts identical stdout
  (`test_matrix([run_via_python, run_via_bin], …)`, 14-38) — a parity suite like LotML's. It
  also accepts divergence: "The Cranelift backend prints a `Bool` as `1`/`0` via `printf`, where
  the Python backend prints `True`/`False`" (`tests/execution.rs:43-47`).
- `tests/README.md` keeps "coverage archaeology": 100% line coverage as the target, the files
  exempt by design, and stub content counted as test surface (`tests/README.md:71-76`).

## Reusable for LotML

| Item | Path in repo | What it gives | LotML crate | License verdict | Effort |
|---|---|---|---|---|---|
| `ruff_python_parser` / `ruff_python_ast` as a library | `Cargo.toml:33-34`, `src/check/context/python.rs:22-33` | parse `.pyi` and annotated `.py` in Rust, no Python process | `lotml` (`bind`), `lotml-py` | dependency, MIT; a new dependency is an ADR plus a `docs/stack.md` entry | M |
| Range-insensitive Python-AST golden comparison | `tests_util/src/lib.rs:246-320`, `419-421` | golden tests of `lotml-py` output that ignore formatting | `lotml-py` tests | adapt the idea (LotML could compare `ast.dump` of its JSON tree) | S |
| Two-backend execution matrix | `tests/execution.rs:14-38` | already present (`compiler/crates/lotml-llvm/tests/common/mod.rs:144-160`) | — | nothing new | — |
| Class-body hoisting into `__init__` | `src/backend/python/convert/class.rs:203-300` | LotML records have no constructor bodies | — | nothing | — |

## Ideas and optimizations worth adopting

1. **A golden test of the generated Python, by tree.** LotML's parity suite checks behaviour; a
   change to `from_ir.rs` that keeps output equal but doubles the work (an extra copy, a lost
   fast path) is invisible to it. A handful of golden programs whose generated tree is compared
   structurally, mamba's way (`tests_util/src/lib.rs:246-320`), would make such changes show up in
   review. No ADR touched. Effort S.
2. **`lotml bind` in-process with ruff's parser.** Mamba reads stubs with `ruff_python_parser`
   (`src/check/context/python.rs:22-33`). `lotml bind` runs `lotml_bind.py` under Python
   (`compiler/crates/lotml-py/src/lib.rs:33-34`). In Rust the bind would be testable without an
   interpreter and could share LotML's type printer. Caveats: typeshed is still found inside an
   installed mypy or jedi, which needs Python; the ruff crates are pre-1.0 with no stability promise
   (mamba pins `0.0.13`, tarvos pins `=0.0.12`). Touches ADR 0012, which rejects reading `.pyi` *at check time* — doing
   it at bind time does not conflict. A new dependency needs an ADR and a stack entry. Effort M.
3. **Never accept a target divergence.** Mamba lets `True` print as `1` natively. LotML's parity
   rule (the Python target is the reference, ADR 0025) should stay absolute: printing, float
   formatting, error text. Already the policy; worth a line in the parity spec.
4. **Write down why lines are uncovered.** `tests/README.md` separates "structural exceptions" from
   "real, closeable gaps"; LotML's test gate holds a coverage floor
   (`harness/scripts/test_gate.py`) and would benefit from the same record. Effort S.

## Pitfalls seen

- **Resources found through `env!("CARGO_MANIFEST_DIR")`** (`src/check/context/resource.rs:53`):
  an installed binary reads its stubs from the source tree of the machine that built it. LotML
  embeds its runtime with `include_str!` (`compiler/crates/lotml-py/src/lib.rs:30-34`); keep it
  that way for anything the compiler ships.
- **Types keyed by source span.** `pos_to_name` unions the types of every node that shares a
  position, and needs a workaround: "trim temp should not be needed, underlying issue with
  current logic" (`src/check/constrain/unify/finished.rs:39`). LotML keys expression types the
  same way, `pub type Types = HashMap<Span, Ty>` (`compiler/crates/lotml-check/src/lib.rs:67`):
  any synthesized or desugared expression that reuses its source's span silently overwrites a
  type. Worth a debug assertion on insert, or node ids, before lowering grows more desugarings.
- **Silent native/Python disagreement**: `Int` wraps at 64 bits natively while Python's does not
  (`src/backend/cranelift/README.md:50-56`), and booleans print differently
  (`tests/execution.rs:43-47`). LotML's integer kinds trap on both targets — keep the parity
  suite covering the edges.
- **Fail-fast parsing** (`src/parse/mod.rs:31-41`): one error per run is the worst case for a
  model repairing its own code; LotML's tolerant parser is the right call.
- **Documentation drift after a syntax change**: indentation removed from the grammar
  (`CLAUDE.md:170`) but still promised in `docs/philosophy/readability.md:33`.
- **Rules written down but not enforced** (`CLAUDE.md:236-239`): `mut` on fields recorded and never
  checked, `mut self` not required for a mutating call.
- **Linking through `cc` only** (`src/backend/cranelift/link.rs:6`) and no variadic-call support
  for `printf` with floats (`src/backend/cranelift/README.md:43-46`): the ABI and toolchain work is where a small
  native backend stalls. LotML sidesteps both by handing `clang` LLVM IR plus a C runtime.
