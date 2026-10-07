# tarvos — Python subset to Rust source to native executable through rustc (`run`/`build`/`compile`)

Repo `repo-tech/Tarvos` · **GPL-3.0** (`LICENSE`, README badge) — ideas only, never code; note
that `pyproject.toml:11` declares `license = { text = "MIT" }`, which contradicts `LICENSE`;
treat the stricter GPL-3.0 as binding · last commit 2026-10-07 · v1.3.1 · Rust: `tarvos-cli` 7.1k
lines, `tarvos-codegen-rust` 6.3k, `tarvos-analysis` 5.3k, `tarvos-core` 3.0k, `tarvos-optimizer`
2.2k, `compiler/` (Ruff frontend) 2.1k, `tarvos-tests` 1.1k, IR/AST/types 0.7k; Python 6.7k
(harnesses, `python/ast_export.py`) · Maturity: young, fast-moving, honest about limits (no
classes, generators, decorators, `re`, `datetime`; `README.md:45-48`), many correctness fixes in
recent releases.

Paths below are relative to `research/prior-art/repos/tarvos/`; LotML paths start with `compiler/crates/lotml`
(tarvos's own `compiler/` directory is its Ruff frontend). A bare `main.rs` or `toolchain.rs` is in
`crates/tarvos-cli/src/`. Under GPL-3.0 nothing here may be copied into LotML; every item below is an idea,
restated in LotML's own terms.

## Architecture

- **Parse**: Ruff's parser through the `compiler/` crate (`ruff_python_parser = "=0.0.12"`,
  `compiler/Cargo.toml`; `compiler/src/ast_bridge.rs:1-2`, `236`), converted to `tarvos_ast`
  (`crates/tarvos-core/src/lib.rs:798-…`). If Ruff reports diagnostics, `TARVOS_COMPAT_AST=1`
  switches to CPython's own `ast` exported as JSON by `python/ast_export.py`, embedded in the
  binary (`crates/tarvos-core/src/lib.rs:38-41`, `504-529`). This is LotML's Python backend in
  reverse: LotML sends a JSON tree *to* CPython (`compiler/crates/lotml-py/src/lib.rs:56-82`).
- **Project**: local imports are inlined into one flat module before lowering
  (`crates/tarvos-core/src/lib.rs:47-56`).
- **Lower** to a tree IR with string names (`crates/tarvos-ir/src/lib.rs:14-145`;
  `crates/tarvos-analysis/src/lower.rs`, 3.0k lines), with type inference and a stdlib table
  (`tarvos-analysis/src/stdlib.rs`, `types.rs`).
- **Optimize** (`crates/tarvos-optimizer/src/lib.rs:14-30`): loop-induction closed form, copy
  propagation, constant folding, propagation and folding again, type specialization, dead code.
- **Emit Rust** (`crates/tarvos-codegen-rust/src/lib.rs`), runtime helpers emitted as Rust source
  into the program only when used (`docs/BENCHMARKS.md:68-73`); a tagged `__TarvosValue` for
  names whose type is not fixed (`crates/tarvos-codegen-rust/src/dynamic_runtime.rs:1-19`).
- **Build** with `rustc` directly (not cargo) into a content-addressed cache, then publish
  (`crates/tarvos-cli/src/main.rs:2290-2370`); a pinned, checksum-verified managed toolchain is
  the default (`crates/tarvos-cli/src/toolchain.rs:322-397`, `688-…`).
- **run / build / compile** share everything up to Rust source: `compile` stops there, `build`
  links with fat LTO, `run` builds with a faster profile and executes; `--python-fallback`
  (alias `--compat-runtime`) runs CPython instead (`main.rs:386-400`).

## Frontend

Ruff's parser; no own lexer. Three parsing paths exist: Ruff (live), CPython's `ast` via
`python/ast_export.py` (fallback), and a `rustpython-parser = "0.4"` dependency in
`crates/tarvos-cli/Cargo.toml:16` that no source file uses. `compiler/ARCHIVED.md:6` says the
`compiler/` directory "is not part of the Cargo workspace", yet `Cargo.toml:3-4` lists it as the
first member and `crates/tarvos-core/Cargo.toml:8` depends on it as the Ruff frontend.

## Semantics and types

- `int` → `i64`, `float` → `f64`, `str` → `String`, `list[T]` → `Vec<T>`, `dict[K, V]` →
  `HashMap` (`docs/subset-contract.md` "Supported type model"); integer arithmetic is checked
  and panics with a Python-like message on overflow (e.g. `checked_pow(...).expect("integer
  power overflow")`, `crates/tarvos-codegen-rust/src/lib.rs:3511`); `range` with a negative step
  materializes a `Vec<i64>` (`crates/tarvos-codegen-rust/src/lib.rs:270-295`).
- Python semantics kept on purpose: floored `//` and `%`, true `/`, negative indexing,
  `IndexError` (`README.md:34`, `60-61`).
- Exceptions: `try`/`except`/`else`/`finally` lower to `Result` values and labelled blocks, after
  a `catch_unwind` design was found "wrong three ways" (`CHANGELOG.md:461-470`).
- Unsupported constructs are refused with a diagnostic, or run on CPython when the user asks
  (`crates/tarvos-core/src/lib.rs:521-526`).

## IR and passes

- One tree IR (`crates/tarvos-ir/src/lib.rs`): `Stmt` with `Let`/`Assign`/`Destructure`/…/`Print`
  (14-…), `Value` with constants, names, typed unary/binary nodes (115-145); `Int128` for
  compile-time reductions that exceed `i64` (118-119).
- `loop_induction_optimization` — `for i in range(a, b): acc = acc + f(i)` replaced by a closed
  form (`crates/tarvos-optimizer/src/lib.rs:36-180`).
- `copy_propagation`, `constant_folding` (with Python sign rules for `%`, 2047-…),
  `TypeSpecializer`, `dead_code_elimination` (199-1560).
- `tarvos-analysis/src/native_detector.rs` — scans NumPy/Pandas loops and proposes a native plan
  or a CPython fallback; analysis only (`docs/native-loop-detector.md`).

## Backend and toolchain

- `rustc` invoked directly. `run`: `-C opt-level=3 -C lto=off -C codegen-units=16`, because LTO
  cost 0.6 s per compile for no run-time gain on a single generated file (`main.rs:2297-2323`).
  `build`: fat LTO, one codegen unit, stripped (`main.rs:2324-2332`). `target-cpu=native` was
  removed after binaries built on new CPUs died with SIGILL on older ones (`main.rs:2333-2340`).
- Output is compiled into a staging file in the cache and published only on success, so a failed
  build never leaves a partial executable (`main.rs:2343-2355`).
- **Toolchain probe**: the managed or system `rustc` must compile and link a trivial program
  once; success is recorded in a stamp file keyed by the compiler's path, version and host target
  and invalidated when the compiler binary is newer than the stamp (`toolchain.rs:140-223`). On
  failure the compiler's own output is passed through, and a missing MSVC `link.exe` on Windows
  is named with the fix (`toolchain.rs:276-318`).
- **Build cost**: transpile 50–105 ms, `rustc` 2.3–4.6 s per tiny program
  (`docs/BENCHMARKS.md:49-60`) — the native toolchain, not the compiler, sets the latency.

## Runtime

No runtime library: helpers are emitted as Rust source per program and dead ones never emitted
(`docs/BENCHMARKS.md:68-73`); `__TarvosValue` (`Int`, `Float`, `Bool`, `Str`, `None`, `List`,
`Tuple`, string-keyed `Dict`) for dynamic names, with one entry point per operator so the
optimizer sees a fixed operation (`crates/tarvos-codegen-rust/src/dynamic_runtime.rs:9-30`). Memory is Rust ownership on
generated code; no counting.

## Testing and conformance

- `benchmarks/difftest.py`: CPython vs native on a corpus, comparing **stdout, stderr and exit
  code**; only nondeterministic addresses are normalized; "A case is only a PASS when the native
  artifact actually runs and matches. Compiler rejections are recorded as SKIP with the reason,
  never as passes" (`benchmarks/difftest.py:1-50`); run in CI (`.github/workflows/ci.yml:428`).
- A machine-generated compatibility matrix (`docs/COMPATIBILITY.md`, `docs/compatibility.json`)
  checked for drift in CI (`.github/workflows/ci.yml:142-145`, `scripts/compatibility_matrix.py`).
- CLI audit of every command (`benchmarks/cli_audit.py`), compatibility gate tests
  (`crates/tarvos-cli/tests/compatibility_gate.rs`).

## Reusable for LotML

Nothing may be copied (GPL-3.0). Ideas only:

| Item | Path in repo | What it gives | LotML crate | License verdict | Effort |
|---|---|---|---|---|---|
| Toolchain probe with a validity stamp | `crates/tarvos-cli/src/toolchain.rs:140-223` | one probe compile, then a cached proof keyed by compiler path, version, target and mtime | `lotml-llvm` (`driver.rs`) | idea only | S |
| Windows linker diagnosis | `crates/tarvos-cli/src/toolchain.rs:276-318` | name the missing linker and the fix instead of a raw link error | `lotml-llvm` (`driver.rs` `rejected`) | idea only | S |
| Content-addressed build cache + staged publish | `crates/tarvos-cli/src/main.rs:2037-2122`, `2290-2370` | reuse unchanged artifacts; never leave a partial output | `lotml-llvm`, `lotml` (`exec.rs`) | idea only | S–M |
| Differential harness rules | `benchmarks/difftest.py:1-50` | stdout + stderr + exit code; SKIP is never PASS | LotML parity suite | idea only | S |
| CI-checked compatibility matrix | `.github/workflows/ci.yml:142-145` | a generated, never-stale list of what the native target supports | `harness`, specs/llvm-parity | idea only | S |

## Ideas and optimizations worth adopting

1. **Cache the compiled C runtime.** Every `lotml build` hands `clang` the program's IR *and*
   `lotml.c` — which `#include`s `lotml_text.c`, `lotml_list.c` and `lotml_dict.c`, about 147 KB
   of C — at `-O2` (`compiler/crates/lotml-llvm/src/driver.rs:164`,
   `compiler/crates/lotml-runtime/c/lotml.c:1047-1049`). They are already separate translation
   units in one `clang` call, so a cached `lotml.o` keyed by clang version, flags
   (`-O0`/`-O2`, `-DLT_COUNT_CELLS`, `-shared`/`-fPIC`, sanitizers) and the runtime's hash loses
   no inlining and removes most of the C compile from every build. Tarvos's numbers show the
   native toolchain dominating build latency (`docs/BENCHMARKS.md:49-60`); an agent loop that
   builds often pays it each time. No ADR conflict (ADR 0021/0025 keep the C runtime compiled by
   `clang`; caching changes when, not how). Effort S–M.
2. **Probe the toolchain once and diagnose the linker.** `driver::find` checks that `clang` exists
   and is new enough (`compiler/crates/lotml-llvm/src/driver.rs:35-60`); a Windows machine with
   LLVM but without the MSVC linker or Windows SDK only fails at the first link, with clang's raw
   output (`compiler/crates/lotml-llvm/src/driver.rs:204-218`). A probe that links a trivial C program, recorded in a stamp keyed
   by the clang path, version and mtime, and a message that names `link.exe`/`lld-link` and the
   SDK, turns a confusing failure into an instruction. ADR 0021/ADR 0022 untouched. Effort S.
3. **Make the parity rules explicit.** Compare stderr (at least the error kind of a trap) and the
   exit code as well as stdout, and count a refusal as SKIP with its reason, never as PASS
   (`benchmarks/difftest.py:1-50`). LotML's harness compares stdout and exit code
   (`compiler/crates/lotml-llvm/tests/common/mod.rs:144-160`). Feeds ADR 0025's parity measure.
   Effort S.
4. **Generate the native-support matrix from the tests** and fail CI when the document drifts
   (`.github/workflows/ci.yml:142-145`). ADR 0025 says the LLVM gap is "measured by the parity
   suite as it closes"; a generated table is that measure in a form a user (or a model choosing
   `run` vs `build`) can read. Effort S.
5. **Publish build outputs atomically.** Build into a temporary file and rename on success
   (`main.rs:2343-2355`), so a failed `lotml build --shared` never leaves a half-written library
   or a stale import library next to a new DLL (ADR 0024). Effort S.
6. **Keep `-march=native` out of shipped builds** (`main.rs:2333-2340`). LotML passes no `-march`
   today (`compiler/crates/lotml-llvm/src/driver.rs:146-169`); worth a line in the build spec so nobody adds it for a benchmark.

## Pitfalls seen

- **Optimizer passes that produce silently wrong programs.** Copy propagation did not learn that
  `a, b = b, a + b` rebinds both names, so a Fibonacci loop compiled to `return 0` — "a silently
  wrong answer rather than a compile error" (`CHANGELOG.md:325-345`, `428-436`). Any pass LotML
  adds to `lotml-ir` (e.g. `hoist.rs`) needs the differential suite, not just unit tests; for
  closed-form loop reductions, leave them to LLVM's scalar evolution at `-O2` rather than writing
  a pattern-matcher like `crates/tarvos-optimizer/src/lib.rs:36-180`.
- **Division by zero**: integer division aborted the process instead of raising a catchable
  error, and float `/` returned `inf` and carried on (`CHANGELOG.md:335-345`). LotML's parity
  corpus should pin both cases on both targets.
- **Exceptions as panics** (`catch_unwind`) cannot carry a class, cannot run `finally` on a
  non-local exit, and do nothing under `panic = "abort"` (`CHANGELOG.md:461-466`). LotML's errors
  as values (ADR 0002) avoid the whole class.
- **A frontend that silently drops syntax**: keyword arguments were discarded, so `f(x, n=2)`
  compiled as `f(x)` (`CHANGELOG.md:492-494`). An unsupported construct must be a diagnostic.
- **Silent fallback**: an unsupported `try` used to send the program to the CPython launcher
  without saying so (`CHANGELOG.md:461-462`). ADR 0025's choice — `build` refuses a Python import
  at the import, with a diagnostic pointing at `lotml run` — is the right one.
- **Unbounded work in a cache key**: hashing every `.py` under the project recursively took
  minutes beside a `transformers` checkout and walked a virtualenv twice through a `lib64 -> lib`
  symlink; now only same-directory
  siblings within a byte budget (`main.rs:2060-2122`).
- **Documentation that contradicts the code**: `docs/subset-contract.md:44-48` calls imports,
  closures and `try` unsupported while `README.md:38-43` lists them as working; `ARCHIVED.md`
  contradicts the workspace; an unused parser dependency; mojibake in optimizer comments from an
  encoding round trip (`crates/tarvos-optimizer/src/lib.rs:33-35`). Their fix — a generated,
  CI-checked compatibility matrix — is idea 4 above.
- **License metadata that disagrees** (`LICENSE` GPL-3.0 vs `pyproject.toml:11` MIT): a reuse
  decision must follow the license text, and the conflict is a reason for extra caution.
