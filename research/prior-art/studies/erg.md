# erg — statically typed, Python-compatible language whose Rust compiler emits CPython bytecode

Repo `erg-lang/erg` · MIT OR Apache-2.0 for code (`Cargo.toml` workspace `license`;
`crates/erg_compiler/lib/LICENCE` is Apache-2.0; `doc/LICENSE` is CC-BY-4.0) · last commit
2025-12-04 · v0.6.53 · Rust: `erg_compiler` 77.7k lines, `erg_parser` 18.6k, `erg_common` 11.8k,
`els` 8.9k, `erg_linter` 0.6k, `src/` 0.5k; Python runtime `crates/erg_compiler/lib/core/*.py`
1.5k; 190 `.d.er` declaration files, 7.6k lines · Maturity: a research language with a large,
working type system, a real LSP and a Python checker built on it (pylyzer), stuck at 0.x, last
commit ten months ago, bytecode backend limited to CPython 3.7–3.11 (`README.md:162`).

Paths below are relative to `research/prior-art/repos/erg/` unless they start with `compiler/` (LotML).
`erg_compiler/…`, `erg_parser/…`, `erg_common/…` and `els/…` are under `crates/`; a bare file
name (`lower.rs`, `ty/mod.rs`, `context/eval.rs`, `build_package.rs`) is in
`crates/erg_compiler/` — except `lex.rs`, `parse.rs`, `desugar.rs`, `build_ast.rs`
(`crates/erg_parser/`), and `python_util.rs`, `io.rs`, `consts.rs`, `levenshtein.rs`
(`crates/erg_common/`).

## Architecture

One frontend, one typed tree (HIR), three emitters. The pipeline, as `Compiler::compile` runs it
(`crates/erg_compiler/compile.rs:276-345`):

1. **Lex** — `erg_parser/lex.rs` `Lexer` (`lex.rs:168-182`): `Vec<char>` + `indent_stack` →
   `TokenStream`.
2. **Parse** — `erg_parser/parse.rs` `Parser` (`parse.rs:210-217`), recursive descent → `AST`
   (`erg_parser/ast.rs`, 7.4k lines).
3. **Desugar** — `erg_parser/desugar.rs` (literal parameters, multiple assignment). `ASTBuilder`
   = parse + desugar behind the `ASTBuildable` trait (`erg_parser/build_ast.rs:12-20`).
4. **Link AST** — `erg_compiler/link_ast.rs` (methods defined across modules).
5. **Lower = type check** — `GenericASTLowerer::lower` (`erg_compiler/lower.rs:3904-3974`):
   `declare` mode for `.d.er` files (3914-3927), `preregister_consts` + `register_defs` for
   forward references (3929-3934), `lower_chunk` per top-level expression keeping partial chunks
   on error (3935-3947), `check_decls`, then `Context::resolve` dereferences every free type
   variable (3954-3964), then lint. Output: `HIR` (`hir.rs`, `Expr` at 3006-3027), every node
   typed.
6. **Effect and ownership checks** — `HIRBuilder::check` (`build_hir.rs:221-245`) runs
   `SideEffectChecker` (`effectcheck.rs`) and `OwnershipChecker` (`ownercheck.rs`).
7. **Link HIR** — `link_hir.rs:37-38`: "Erg links all non-Python modules into a single pyc file."
8. **Desugar HIR** — `desugar_hir.rs`: class static members become attribute assignments, "to
   make it more like Python semantics".
9. **Optimize** — `optimize.rs`: dead-variable elimination only; `_fold_constants` is `todo!()`
   (`optimize.rs:26-28`).
10. **Emit** — `codegen.rs` `PyCodeGenerator` (214, `emit` 4045) → `CodeObj` → `.pyc`
    (`ty/codeobj.rs:460-481`); or `transpile.rs` `PyScriptGenerator` (423) → Python source; or
    `JsonGenerator` (1265).

Package level: `GenericPackageBuilder<ASTBuilder, HIRBuilder>` (`build_package.rs:180-197`)
resolves imports into a module graph and checks each imported module on its own thread
(`build_package.rs:980-981`), all sharing `SharedCompilerResource` (`module/global.rs:25-38`:
`mod_cache`, `py_mod_cache`, `index`, `graph`, `trait_impls`, `promises`, `errors`,
`gen_cache`).

**Run vs build.** There is no native target. `erg run` compiles to bytecode, writes a `.pyc` and
runs CPython on it with `exec(marshal.loads(open(...).read()[16:]))`
(`erg_common/python_util.rs:828-840`); `erg --compile` leaves the `.pyc` for Python to import;
the REPL (`src/dummy.rs:182-216`) compiles each input and sends it over TCP to one persistent
Python process started from `src/scripts/repl_server.py`. All three share the frontend and HIR;
the transpiler shares everything up to the HIR but is a second-class emitter (see Pitfalls).

**Against LotML.** LotML has one IR between the checker and both targets (ADR 0020) and two
targets (ADR 0025). Its Python backend sits between erg's two emitters: it writes Python's own
syntax tree as JSON, every node at its LotML position, and the runtime calls `compile()` on it
(`compiler/crates/lotml-py/src/lib.rs:56-82`, `compiler/crates/lotml-py/runtime/lotml_rt.py:50-68`).
That keeps erg's advantage of bytecode (no text to re-parse, positions that point at the source
language) without its cost (one emitter per CPython version, below).

## Frontend

- **Indentation.** `indent_stack` holds increments, validated against cumulative sums so a dedent
  must land on an enclosing level (`lex.rs:504-574`); one `Dedent` per `next()` call, emitted by
  rewinding the cursor (`self.cursor -= spaces_len`, 546); more than 100 spaces is an error,
  "same as the CPython's limit" (506-528); an indented first line is an error (481-494); tabs are
  rejected, "cannot use a tab as a space" (1487); `enclosure_level` suppresses `Newline` inside
  brackets (465-470). LotML's lexer does the same job (`compiler/crates/lotml-syntax/src/lexer.rs:450-490`)
  and accepts tabs as the next multiple of four (311). Nothing to take.
- **Parser recovery.** "the parsing process will continue as long as it's not fatal"
  (`parse.rs:205-209`); recovery skips to the next separator, the next line, or the matching
  dedent (`next_expr` 277-292, `next_line` 294-307, `until_dedent` 309-330). But a lexical error
  discards the token stream and returns no AST at all: `.map_err(|(_, es)| es)?`
  (`parse.rs:221`, again at 487-489).
- **Partial results.** `IncompleteArtifact { object: Option<Inner>, errors, warns }`
  (`erg_compiler/artifact.rs:26-30`) lets the language server keep a HIR when checking fails.
- **Patterns** are parsed as expressions and converted afterwards (`erg_parser/convert.rs`).
- **Incrementality.** No salsa. The language server re-checks a whole file and then every
  dependent file recursively (`els/diagnostics.rs:137-246`, dependents at 237-242). On an edit it
  first tries `quick_check_file` (`els/diagnostics.rs:262-306`): it diffs the old and new
  top-level expression lists by index (`els/diff.rs` `ASTDiff::{Deletion, Addition,
  Modification, Nop}`) and re-lowers only the changed chunk into the existing HIR.
- **REPL continuation** is detected from the parser's `ExpectNextLine` error kind
  (`erg_parser/build_ast.rs:72-100`).

## Semantics and types

- **Context-based checker**: one `Context` per module, class and function
  (`context/mod.rs`; `register.rs` 3.4k lines, `inquire.rs` 4.6k, `compare.rs` subtyping 2.7k,
  `unify.rs` 2.4k, `generalize.rs` 1.9k, `instantiate*.rs` 4.2k, `eval.rs` constant evaluation
  4.5k).
- **Types** (`ty/mod.rs:1388-1471`): nominal monotypes, `Poly { name, params }`, `Subr`,
  `Record`, `Refinement`, `Quantified`, `And`/`Or`/`Not`, `Proj`/`ProjCall` (associated types),
  `Structural`, `Guard` (narrowing by `isinstance`, 1458-1460), `Bounded`, `FreeVar`, and
  `Failure`, which "behaves as `Any`" after an inference failure (1467-1468) — the same trick as
  LotML's `Ty::Error` (`compiler/crates/lotml-check/src/ty.rs:53-54`).
- **Inference**: free type variables with levels for let-generalization and two-sided bounds,
  `Constraint::Sandwiched { sub, sup }` (`ty/free.rs:56-68`); `FreeKind::UndoableLinked`
  (`ty/free.rs:292-310`) lets speculative unification be rolled back. `doc/EN/compiler/inference.md`
  explains the level discipline.
- **Refinement and dependent types**: `RefinementType { var, t, pred }` (`ty/mod.rs:1056-1060`)
  over `Predicate` (`ty/predicate.rs:18-58`); `Nat`, literal sets `{0, 1, 2}` and ranges `1..10`
  all become refinements (`ty/mod.rs:1427-1433`); sized lists `[Int; 3]`
  (`tests/should_ok/dependent.er`).
- **Generics** are not monomorphized — CPython is the target, so polymorphic code runs as is.
- **Effects**: procedures end in `!`; `effectcheck.rs:30-33` rejects side effects inside
  functions and state-changing methods on immutable classes.
- **Ownership**: values of mutable types (`T!`) are moved when passed to an owned parameter
  (`Type::args_ownership`, `ty/mod.rs:879-893`); `ownercheck.rs` reports use after move
  (`check_acc` 293-303, `check_if_dropped` 369-388). It is flow-insensitive — `drop` removes the
  variable from the enclosing scope even when the move sits inside one branch's lambda
  (356-367) — and closures are `// TODO: capturing` (276). A Rust-style ownership system was
  planned and abandoned "due to its incompatibility with Python's semantics and the need to
  introduce complicated specifications such as lifetime annotations"
  (`doc/EN/compiler/abandoned.md:7-10`). That is the same conclusion ADR 0008 reached.
- **Python's dynamism**: an undeclared Python import is typed `Object` and must be narrowed
  (`doc/EN/syntax/34_integration_with_Python.md:27`); overloads exist only for Python
  declarations, as an intersection of subroutine types (same file, "Overloading").

### Python modules: `.d.er` declarations against ADR 0012

- Declarations are Erg type syntax, one per member: `.sqrt: Float -> Float`
  (`crates/erg_compiler/lib/pystd/math.d.er`). The standard library's are hand-written — "All
  APIs in the Python standard library are type-specified by the Erg development team" (`34_integration_with_Python.md:31`) — 116 entries in `lib/pystd`, 190 files and 7.6k lines in all.
- "Type hints on the Python side are ignored" (`…:40`). "Currently, Erg unconditionally trusts
  the contents of type declarations" (`…:114`): code generation drops type ascriptions
  (`codegen.rs:3407`, `3453`) and nothing checks a returned value.
- Search order: `{path}.d.er`, `{path}/__init__.d.er`, `__pycache__/{to}.d.er`, the `.d`
  directory forms, `std/`, `pkgs/`, `site-packages` (`erg_common/io.rs:626-637`).
- A Python module without one gets one generated: the compiler spawns `pylyzer --dump-decl
  <file.py>`, which infers declarations from the Python *source* and writes them under
  `__pycache__` (`build_package.rs:616-659`). The file starts with a status line `##[pylyzer]
  succeed foo.py <timestamp> <hash>` whose "hash" is the file length (`build_package.rs:84-142`,
  `608`); a concurrent reader busy-waits up to 600 × 100 ms while the file is empty
  (`build_package.rs:697-703`).
- Python runs at check time: `get_sys_path` shells out to read `sys.path`
  (`python_util.rs:796-826`), and the magic number comes from the target interpreter
  (`python_util.rs:653-679`).

ADR 0012 differs on every point and names Erg among the rejected options ("Hand-written
declarations, as Erg and Codon require"): interfaces are generated from typeshed, a call returns
`T ! PyError`, the returned value is checked at run time (`lotml_rt.py:530-549`), and the checker
reads only `.lotmli` text (`compiler/crates/lotml-check/src/interface.rs:121-180`). Erg is the
evidence that the rejected option costs thousands of hand-kept lines and still trusts blindly.

### The shared-crate pattern (els, pylyzer)

- `PYTHON_MODE = cfg!(feature = "py_compat")` (`erg_common/consts.rs:6`) switches about 139
  sites across the crates so the same compiler checks Python: pylyzer converts Python to an Erg
  AST and plugs in through the `ASTBuildable` generic of the package builder
  (`build_package.rs:180-197`).
- The language server is generic over its checker: `Server<Checker: BuildRunnable =
  PackageBuilder, Parser: Parsable = SimpleParser>` (`els/server.rs:198-200`), so pylyzer reuses
  it whole by substituting its checker.
- LotML's LSP and MCP live in the `lotml` crate over `lotml-db` and `lotml-ide`; with one
  frontend there is nothing to parametrize. The pattern is worth knowing only if LotML ever
  checks a second source language (a Python subset) with its checker.

## IR and passes

- `AST` (`erg_parser/ast.rs`) → desugared AST (`desugar.rs`) → linked AST (`link_ast.rs`).
- `HIR` (`erg_compiler/hir.rs`), a typed tree:
  - `lower.rs` — inference and lowering in one walk.
  - `effectcheck.rs` — purity of functions.
  - `ownercheck.rs` — moves of mutable objects.
  - `lint.rs` — lints that later optimizations depend on; `erg_linter` (551 lines) the rest.
  - `link_hir.rs` — Erg modules inlined into one code object.
  - `desugar_hir.rs` — class statics to attribute assignments.
  - `optimize.rs` — discarded and unused variables when `opt_level > 0` outside the REPL
    (`optimize.rs:18-24`, `75-78`).
- No CFG, SSA, monomorphization, counting or reuse. LotML's `lotml-ir` already has more
  (`mono.rs`, `own.rs`, `reuse.rs`, `hoist.rs`, `verify.rs`).

## Backend and toolchain

- **Bytecode**: `codegen.rs` (4.1k lines) writes CPython code objects directly. It carries 59
  `py_version.minor` branches (e.g. `codegen.rs:394-441`) and one opcode table per version
  (`erg_common/opcode308.rs` … `opcode311.rs`); there is no 3.12 table and no reference to 3.12
  anywhere. The `.pyc` is magic number + padding + timestamp + padding + marshalled code
  (`ty/codeobj.rs:471-481`).
- **Transpiler**: `transpile.rs` builds strings. Runtime support is pasted into each output with
  `include_str!` of `lib/core/*.py` and string surgery on their imports (`replace_import`, "TODO:
  more smart way", 470-492); a multi-statement `if`/`match` expression becomes a helper `def`
  appended to the module prelude, "FIXME: this trick only works in the global namespace"
  (888-913, 916-995); several arms are `todo!("transpiling …")` (594, 606, 633, 665).
- **Finding Python**: `[tool.erg.python] path` in `pyproject.toml` (`python_util.rs:571-593`),
  then `./.venv/bin/python` (619-621, the Unix layout only), then the poetry environment
  (595-613), then `where python` on Windows or `which python3` (625-646); pyenv-win is refused
  (642-644). Commands are composed through `cmd /C` and `sh -c` with interpolated paths
  (806-818) and the `.pyc` path is spliced into Python source (837-839).
- No native code, JIT or shared libraries. A `pylib` feature exposes the compiler to Python
  through pyo3 (`erg_compiler/lib.rs:40-80`).

## Runtime

- Values are CPython objects. Erg's builtins are subclasses that re-wrap every result:
  `class Int(int)` with `__add__` returning `then__(int.__add__(self, other), Int)`
  (`lib/core/_erg_int.py:6-40`) — a Python-level call per arithmetic operation in transpiled
  code.
- `Result` is an untagged union: `Error` is a plain class and `is_ok(obj)` is `not
  isinstance(obj, Error)` (`lib/core/_erg_result.py:4-22`); success values are returned bare.
  LotML wraps both sides in frozen dataclasses (`lotml_rt.py:95-101`), so every successful
  fallible call allocates an `Ok`.
- Records become `namedtuple`s (`doc/EN/compiler/transpile.md`). Memory is CPython's.

## Testing and conformance

- `tests/should_ok` (77 programs), `tests/should_err` (55), `tests/eval`, driven by
  `tests/test.rs`. `expect_compile_failure(path, num_warns, num_errs)` asserts only the *number*
  of errors (`tests/common.rs:148-180`); `expect_error_location_and_msg` checks locations and
  messages for a few programs (`tests/common.rs:182-…`).
- Parser token tests (`crates/erg_parser/tests/tokenize_test.rs`), language-server tests
  (`crates/els/tests/test.rs`), REPL tests (`tests/repl.rs`).
- No differential testing against CPython: Erg's semantics differ from Python's by design.

## Reusable for LotML

| Item | Path in repo | What it gives | LotML crate | License verdict | Effort |
|---|---|---|---|---|---|
| `.pyc` layout and marshal loader | `crates/erg_compiler/ty/codeobj.rs:460-481`, `crates/erg_common/python_util.rs:828-840` | the recipe for caching a compiled code object per interpreter (magic, flags, marshal) | `lotml-py/runtime/lotml_rt.py` (`load`) | idea only — LotML would do it in Python with `marshal` and `importlib.util.MAGIC_NUMBER` | S |
| Interpreter discovery: `pyproject` key, `.venv`, poetry | `crates/erg_common/python_util.rs:571-647` | the project's own interpreter, so `lotml run` sees its installed packages | `lotml-py/src/lib.rs` (`python()`, 36-49) | adapt (MIT/Apache, attribute); add the Windows `.venv\Scripts\python.exe` layout erg misses | S |
| Declaration generation from a module's source (`pylyzer --dump-decl` flow) | `crates/erg_compiler/build_package.rs:616-659` | the shape of "no stub → derive one from the package" | `lotml-py/runtime/lotml_bind.py` | idea only (pylyzer is a separate project) | S |
| Top-level AST diff for quick re-check | `crates/els/diff.rs`, `crates/els/diagnostics.rs:262-306` | re-check one changed item instead of the file | `lotml-db`, `lotml-ide` | idea only — salsa queries per item are the LotML way | M |
| Hand-written stdlib declarations | `crates/erg_compiler/lib/pystd/*.d.er` | a second opinion to cross-check generated `.lotmli` signatures in tests (e.g. `math.factorial: (n: Nat) -> Nat`) | `lotml` bind tests | data under Apache-2.0 (`lib/LICENCE`); reading suffices | S |
| Levenshtein "did you mean" | `crates/erg_common/levenshtein.rs:6-60` | already present in LotML's diagnostics (admissible alternatives, `compiler/crates/lotml-diag/src/lib.rs:1-7`) | — | nothing new | — |

## Ideas and optimizations worth adopting

1. **Bind from the installed package when no stub exists.** Erg falls back to declarations derived
   from the module's source (`build_package.rs:616-659`); `lotml bind` without `--stub` looks
   only for typeshed's stdlib copy inside mypy or jedi
   (`compiler/crates/lotml-py/runtime/lotml_bind.py:166-187`). Most third-party packages a `lotml
   run` program would import (ADR 0025 names numpy and FastAPI) ship a `.pyi` beside the module
   or inline annotations with `py.typed`; Python's `ast` reads an annotated `.py` exactly like a
   stub. Resolve `importlib.util.find_spec(module)`, prefer a sibling `.pyi`, else read the
   annotated source; an unannotated function is listed as unsupported, like `Any`. Touches
   ADR 0012, which says the bind "reads a stub with Python's own parser" — an extension, not a
   conflict, as long as it stays at bind time, generated, checked in and checked at run time.
   Effort S.
2. **Use the project's interpreter.** `lotml_py::python()` takes `LOTML_PYTHON`, then the first
   `python3`, `python`, `py -3` on `PATH` (`compiler/crates/lotml-py/src/lib.rs:36-49`). A
   project with a `.venv` or an active `VIRTUAL_ENV` runs `lotml run` against the wrong
   site-packages unless the user sets the variable. Add `VIRTUAL_ENV`, then `.venv/bin/python` /
   `.venv\Scripts\python.exe` walking up from the source file, before `PATH` (erg:
   `python_util.rs:571-647`, minus its Unix-only `.venv` path and its `where python`, which on
   Windows can return the Store alias LotML already guards against). Touches ADR 0025 and
   ADR 0012; no conflict. Effort S.
3. **Cache the compiled code object.** `lotml_rt.load` decodes the JSON payload, rebuilds the
   `ast` tree and calls `compile()` on every import (`lotml_rt.py:50-68`). Erg's answer is a
   `.pyc` (`codeobj.rs:460-481`). Keep the JSON tree as the interchange — it is what frees LotML
   from per-version bytecode — and cache `marshal.dumps(code)` keyed by the payload hash and
   `importlib.util.MAGIC_NUMBER` next to the generated module. Agent loops re-run unchanged
   modules constantly. No ADR touched. Effort S; measure the saving first on a large program.
4. **Unboxed success for `T ! E` on the Python target.** Erg returns success values bare and
   wraps only errors (`_erg_result.py:4-22`). LotML could construct `Err` only and return `T`
   directly, testing `isinstance(v, Err)` where `Expr::ResultIsOk` now tests `Ok`
   (`compiler/crates/lotml-py/src/from_ir.rs:1180-1184`). It is only sound when `T` cannot itself
   hold an `Err` — a nested `(T ! E) ! F` keeps its tag — so the choice is type-directed in
   `from_ir.rs`. Same semantics under ADR 0002, fewer allocations; measure first, and run the
   whole parity suite, because the Python target is its reference. Effort M.
5. **Range facts as an IR analysis, not as types.** Erg turns `Nat`, literal sets and ranges into
   refinement types (`ty/mod.rs:1427-1433`, `ty/predicate.rs`); mamba's native backend notes the
   same need (`research/prior-art/repos/mamba/src/backend/cranelift/README.md:64-67`). Exposing refinement
   types would cut against LotML's small type system (ADR 0004, `docs/wiki/pages/type-system.md`); an interval
   pass over `lotml-ir` that removes overflow traps and bounds checks it can prove (an index from
   `range(len(xs))`) would not. LLVM at `-O2` already removes some; benchmark before building.
   Touches ADR 0016/ADR 0021 only as an optimization. Effort M–L.
6. **Item-level incrementality.** Erg re-lowers only the changed top-level chunk
   (`els/diagnostics.rs:262-306`); LotML's salsa queries are per file
   (`compiler/crates/lotml-db/src/lib.rs:28-47`). Per-function `checked` queries would make the
   LSP and MCP server cheaper on long files. Effort M–L.
7. **A warm Python for repeated runs.** Erg's REPL keeps one Python process and ships code to it
   (`src/dummy.rs:182-216`); every `lotml run` and `lotml test` invocation starts its own
   interpreter (`compiler/crates/lotml/src/exec.rs:93`, `385-400`), and an agent loop invokes
   them over and over. A worker that loads each program into a fresh
   namespace (`lotml_rt.load_module` already builds one) would save interpreter start-up, at the
   price of state leaking through imported modules. Lowest priority. Effort M.

## Pitfalls seen

- **Bytecode ties the compiler to CPython versions.** 3.7–3.11 only (`README.md:162`), an opcode
  table per version (`erg_common/opcode308.rs` … `opcode311.rs`), 59 version branches in
  `codegen.rs`, nothing for 3.12 at the last commit. LotML's JSON-tree-plus-`compile()` route
  (ADR 0001, ADR 0025) avoids this; do not trade it for bytecode.
- **Trusted declarations.** A declaration that lies produces wrong values silently
  (`34_integration_with_Python.md:114`). ADR 0012's run-time check is the fix.
- **Python at check time.** `sys.path` queries, magic-number detection and a pylyzer subprocess
  with a 60-second busy-wait (`build_package.rs:697-703`) make checking slow and dependent on the
  machine. ADR 0012 already rules it out; keep `lotml check` Python-free.
- **Error numbers are compiler line numbers.** `errno` is `line!()` at the call site
  (`context/eval.rs:781-783`), printed as `Error[#2223]` (`erg_common/error.rs:990-1003`), so
  every edit to the compiler renumbers its errors. LotML's stable codes with explanation pages
  (`compiler/crates/lotml-diag/src/codes.rs`) are what an agent needs; keep them.
- **A second emitter off the shared IR rots.** The transpiler pastes runtime files with string
  replacement and hoists helpers that only work at module level (`transpile.rs:470-492`,
  `888-913`), with `todo!()` arms. ADR 0020 exists to prevent exactly this.
- **A lexical error loses the AST** (`parse.rs:221`, `487-489`). A tolerant parser must carry
  tokens through lexical errors, or editors and agents get nothing back.
- **Flow-insensitive move checking** (`ownercheck.rs:356-367`) and an abandoned ownership system
  (`doc/EN/compiler/abandoned.md:7-10`) — more support for ADR 0008's "no visible borrow
  checker".
- **Type-system complexity has a price**: "adding a new variant above `Poly` may cause a
  subtyping bug, possibly related to enum internal numbering, but the cause is unknown"
  (`ty/mod.rs:1440-1441`). LotML's `Ty` (`compiler/crates/lotml-check/src/ty.rs:25-55`) is small
  on purpose.
- **Shell-composed commands** with interpolated paths (`python_util.rs:806-818`, `837-839`) break
  on quotes and spaces. LotML passes paths as JSON strings and arguments as vectors
  (`compiler/crates/lotml-py/src/lib.rs:72-80`, `compiler/crates/lotml-llvm/src/driver.rs:1-3`).
- **Error-count oracles** (`tests/common.rs:148-180`) pass when one error is replaced by another.
- **Planned optimizations that never land**: constant folding is still `todo!()`
  (`optimize.rs:26-28`).
