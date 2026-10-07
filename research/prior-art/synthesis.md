# Prior-art study — synthesis

2026-10-07. Sixteen projects that run or compile Python, or a Python-derived language, mostly
with a compiler in Rust, read against LotML's compiler (`compiler/crates/`) and its ADRs. One
study per repo is in `studies/`; the method and template are in `BRIEF.md`. Clones are in
`repos/` and DeepWiki exports in `deepwiki/`. All of `prior-art/` is local only, through
`.git/info/exclude`, and none of it is committed.

## Verdict

None of the projects has a compiler core worth importing into LotML. LotML's own pieces are
already sounder:

- **IR:** structured, typed and monomorphised, with ownership and reuse passes (`lotml-ir`).
- **Arithmetic:** exact Python semantics. pon's optimising tier and RustPython's JIT both lower
  `//` and `%` wrongly.
- **Runtime:** a counting C runtime with CPython-exact hashing, dict order and float repr.
- **Toolchain:** textual LLVM IR compiled by `clang`. pycc paid heavily for inkwell plus a
  pinned LLVM. pon and plix have no working Windows path.

What the projects offer is around the core rather than in it:

- how to prove the two targets agree (floors, fuzzing, data-driven conformance);
- how to make builds and checks fast (runtime object cache, finer salsa queries);
- a handful of runtime algorithms;
- a long list of pitfalls they paid for and LotML has not hit yet.

The evidence confirms adr:0020, adr:0021 and adr:0025 rather than challenging them.

## Defects found in LotML while reading

Each was checked against the source in this session. The parser bug has not been reproduced yet.

1. **The C runtime is recompiled on every native build and every LLVM test.**
   `lotml-llvm/src/driver.rs:164` passes `lotml.c` to clang on each build. A subagent timed it:
   about 250 ms at `-O0` and about 915 ms at `-O2`, or 60–80 % of a small build.
   (mun, plix, erg/tarvos, starlark studies)
2. **Every edit re-checks the whole file, and interfaces are re-parsed on every check.**
   `lotml-db/src/lib.rs:34-39` makes the whole file one `checked` query. `interfaces()` at
   `:21-26` is not a tracked query. (ruff/ty, mun studies)
3. **An unclosed `(` hides the rest of the file.** The lexer drops newlines and indentation while
   the bracket depth is above 0 (`lotml-syntax/src/lexer.rs:405,416`), so every later `fn`
   disappears from the outline and from symbol-addressed edits. Ruff re-lexes on recovery
   (`ruff_python_parser/src/lexer.rs:1222-1332`). (ruff study)
4. **Every reference-count increment is an out-of-line call.** `lt_inc` is declared inline in
   `lotml.h:105`, but the `.ll` calls the extern copy in `lotml.c:7` (`emit.rs:1813`), with no
   LTO. (pon study)
5. **`s[i]` is O(n) on non-ASCII text, so index loops over it are quadratic.** `lt_offset` walks
   the string (`lotml_text.c:110`). RustPython's `Wtf8Index` gives O(1) indexing.
   (RustPython study)
6. **The recursion limit differs between targets.** The Python target stops at CPython's 1000
   frames; native code stops when the OS stack runs out (1 MiB on Windows). No limit is set on
   either side. (monty study)
7. **Expression types are keyed by span** (`lotml-check/src/lib.rs:67`). Any desugared node that
   reuses a span silently overwrites a type. mamba hit this and needed a workaround.
   (mamba study)
8. **CI tests only on Ubuntu.** The Windows release job only runs `--version`
   (`.github/workflows/release.yml`). plix shipped a broken Windows calling convention for this
   same reason. (plix study)

## Recommendations, ranked

### A — high impact, small, no ADR conflict

| # | What | Evidence | LotML touchpoint |
|---|---|---|---|
| A1 | **Cache the runtime object.** Key it on the runtime sources, clang version, opt/sanitizer/shared flags and target. | mun builds its runtime once; plix's `build.rs:57-87` fingerprints the toolchain | `lotml-llvm/src/driver.rs`; note in adr:0025, whose wording is "compiled with every program" |
| A2 | **Parity floor.** A committed list of passing programs plus a minimum count. Each result is classed pass/fail/refused/error, and refusals are grouped by message. Only the floor fails CI. | pon `pon-conformance/src/ratchet.rs`, `aot.rs`; lpython lost 188/400 tests silently to CMake comments | `harness/lotml_harness/experiments/parity.py` already classes each program; it lacks the committed floor and a CI gate. All 509 programs report `same` today |
| A3 | **Two-target differential fuzzer.** Generate typed programs (templates plus `lotml dev mutate`) and use `lotml run` as the oracle against `lotml build`. Batch many cases into one `main`, compare error class rather than bytes, and minimise by chunk deletion. Also build with cell counting and `LOTML_SANITIZE`. | pon `fuzz.rs`; SPy's hypothesis two-pipeline test; plix's seeded generator; depyler is the anti-pattern (oracle written in Rust, 0/30 matches) | new harness tool; specs |
| A4 | **Fuzz the frontend for panics.** Feed arbitrary text to parse, check and lower, since the LSP and MCP servers receive it. | depyler, ruff | `lotml-syntax`, `lotml-check`, `lotml-ir` |
| A5 | **Re-lex on bracket recovery** (defect 3), plus a guard that the parser always advances and tests that spans are ordered and nested. | ruff parser | `lotml-syntax` |
| A6 | **Windows runner for the LLVM parity tests.** | plix, pon | `.github/workflows/ci.yml` |
| A7 | **One clang probe** that names a missing `link.exe` or Windows SDK instead of passing clang's raw error through. | tarvos (idea only) | `lotml-llvm/src/driver.rs` |

### B — high impact, medium effort

| # | What | Evidence | Touchpoint / ADR |
|---|---|---|---|
| B1 | **Finer salsa queries.** Per-function `check_fn` queries with item-relative spans, and interfaces as tracked inputs with high durability. Needed for the under-100 ms check on every edit. | ty runs on the same salsa 0.28 (`ty_python_semantic/src/types/infer.rs`, `ruff_db/src/files.rs:139-205`); mun's signature/body split | `lotml-db`, `lotml-check` |
| B2 | **Inline the counting fast path**, either directly in the emitted IR or through `-flto`. | pon: a helper call per operation is the cost | `lotml-llvm/src/emit.rs`. `-flto` with `lld` touches adr:0021 |
| B3 | **Constant collection literals as static count-0 cells**, copied on first write as string literals already are. | starlark `ListOfConsts`/`DictConstKeys`; `emit.rs:1907-1914` does a push per element | `lotml-ir`, `lotml-llvm`, runtime; fits adr:0003/0008 |
| B4 | **Fold and inline in `lotml-ir`.** Fold only results that cannot fail, so overflow and division by zero stay run-time errors. Inline `return <expr>` bodies under a size cap, and mark builtins pure. Test with folding on and off. | starlark (safe recipe); SPy and lpython both moved errors from run time to compile time | `lotml-ir`; benefits `lotml run`, which gets no LLVM `-O2` |
| B5 | **One operator table** `(op, lhs type, rhs type)` -> implementation name, read by the checker and every backend. | SPy opimpl; today the rule lives in `lotml-check/src/body.rs:1704-1778`, the IR, and each backend | `lotml-check`, `lotml-ir`; fits adr:0020 |
| B6 | **Warm CPython worker pool** behind `lotml run`/`test`, the MCP `test` tool and the grader. | monty's sub-millisecond latency comes from its pool, not its interpreter | `lotml` exec, harness; fits adr:0025 |
| B7 | **Resource metering set in the IR**: an op budget charged on back-edges and calls, and a recursion limit that is the same on both targets (defect 6). | edge-python `vm/dispatch.rs:403-410`; monty checks every 255 steps at about 2 % cost | needs a spec (new language behaviour) |

### C — worth having, lower priority

- **Data-driven conformance matrix:** each feature row cites the fixtures that prove it and what
  they leave out (core gap or out of scope). The status is derived mechanically from that.
  (pycc `docs/PYTHON_STANDARDS.md`)
- **Snapshot tests that pin inferred types and IR**, not only diagnostic codes. (mun)
- **Named IR passes**, each with a dump after it and per-pass timing; prune unreachable
  functions before clang; add a `visit.rs` (`operands_mut`, `walk_block`). (lpython)
- **PE/ELF readers in tests**, to assert that a native program never needs libpython and that a
  `--shared` library exports exactly its C ABI set. (pycc `src/embed/pe.rs`, `elf.rs`, MIT)
- **Link arguments as a pure function of the target triple**, plus `-Wl,-Bsymbolic` for
  `--shared` on Linux. (pycc `src/ext_build.rs:288-402`)
- **Compile-time f-string format-spec check.** (RustPython `common/src/format.rs:327`)
- **`lotml bind` improvements:**
  - read the package's own `.pyi` or annotated `.py` when typeshed has no stub (erg);
  - use the project venv's interpreter (erg);
  - read stubs without Python, with `ruff_python_parser` and an embedded typeshed (monty). This
    one needs an amendment to adr:0012 and a `stack.md` entry, and the ruff crates are 0.0.x.
- **Cache the compiled code object** in `lotml_rt.py`, instead of JSON, then AST, then
  `compile()` on every load. (erg)
- **Small dicts without an index up to 16 entries.** Dicts only; sets must keep CPython's slot
  order. (starlark)
- **A `lotml check` speed floor** taken as the minimum of 5 samples; a changed-line coverage
  report. (pycc)

## Where each recommendation went

| recommendation | plan |
|---|---|
| A2, A3, A6, defect 6 (recursion depth), the conformance matrix and PE/ELF checks from C | `plans/target-parity-assurance.md` |
| A4, A5, defect 3 (unclosed bracket), defect 7 (span-keyed types), the f-string spec check from C | `plans/frontend-robustness.md` |
| A1, A7, B1, defect 1 (runtime recompiled), defect 2 (whole-file check) | `plans/build-and-check-speed.md` |
| B2, B3, B6, defects 4 and 5, Ryu, Timsort, small dicts and the code object cache from C | `plans/runtime-hot-paths.md` |
| B4, B5, the named passes, pruning and snapshot tests from C | `plans/ir-passes.md` |
| the `lotml bind` items from C | `plans/bind-sources.md` |
| B7 beyond recursion depth (operation budgets, memory limits) | not planned: a language decision for the owner |
| the `lotml check` speed floor and changed-line coverage from C | not planned |

## Code that can be copied (license-clean)

| Item | From | License | Into |
|---|---|---|---|
| Ryu float-to-string plus a normaliser to CPython's repr (`spy/libspy/src/str.c:108-203`, vendored Ryu) | SPy | MIT; Ryu is Apache-2.0/BSL-1.0, keep its license files | replaces the 17-iteration `snprintf`/`strtod` loop in `lt_shortest` (`lotml.c:551-569`) |
| `Wtf8Index` code-point index (`common/src/wtf8_index.rs`) | RustPython | MIT | ported to C for `lt_offset` (defect 5) |
| Timsort (`crates/vm/src/sorting.rs:649`), which follows CPython's `listsort` | RustPython; port from CPython's `Objects/listobject.c` | PSF-2.0 | ported to C, replacing the runtime's merge sort |
| `FormatSpec` parser (`common/src/format.rs:327`) | RustPython | MIT | `lotml-check` |
| Error-recovery and re-lex techniques | ruff | MIT | `lotml-syntax`, adapted rather than copied |
| PE/ELF readers, triple-to-link-arguments table | pycc | MIT | test helpers, `lotml-llvm/src/driver.rs` |
| ctypes struct marshalling (`lpython.py:425-477`) | LPython | BSD-3 | only after a new ADR, since adr:0013 rules out structs for now |

**Do not swap the parser for ruff's.** The languages differ at the token level (`fn`, `var`,
`fail`, `T ! E`, `??`). Ruff's AST is generated and declared unstable. Adopting it means keeping
about 50k lines of fork in place of LotML's ~3.4k. RustPython, which is Python, forks it anyway
and adds 2.6k lines.

## Evaluated and rejected — the ADRs hold

| Idea | Seen in | Conflicts with | What the evidence says |
|---|---|---|---|
| Cranelift backend or JIT for fast builds | pon, plix, RustPython | adr:0021, adr:0025 | The runtime, not `.ll` compilation, dominates build time (defect 1). Plix's native code is slower than CPython on calls. |
| inkwell / linked LLVM / embedded LLD | pycc, mun | adr:0021 | pycc: pinned LLVM, vcpkg libxml2, verify skipped on Windows, a non-relocatable binary |
| Tracing GC | pon, mun | adr:0003, adr:0008 | pon pays a write barrier per store, shadow stacks, and no precise maps in AoT |
| An interpreter for `run` (Rust, Starlark-style, RustPython) | monty, starlark, RustPython | adr:0025 | 5–8k lines plus a third copy of every builtin; even monty keeps execution out of process |
| An IR shared with another language | lpython/LFortran | adr:0020 | After a libasr sync, 188/400 tests were commented out, including classes, generics and interop |
| Bytecode emission | erg | adr:0001, adr:0025 | One opcode table per CPython version; stuck at 3.7–3.11 |
| Redshift, blue functions, generics checked at instantiation | SPy | adr:0004, checked generics | Type errors are reported lazily |
| Records passed to C as counted handles | mun | adr:0024 (explicitly rejected) | — |
| A CPython extension from `build --shared` | pycc's pivot | adr:0025 | Would reopen closed scope; needs a new ADR |

## Process lessons for an agent-built project

- **pycc**, in about 75 days, accumulated 255 decision records, 7.1 MB of docs, ~60k lines of
  governance scripts and 206 test files named after issues. Its D-242 decision rolled much of
  that back because it was eating each run's tokens. Its architecture doc describes crates that
  do not exist.
- **pycc's HIR has no source positions** (`Span::new(0,0)` at 207 sites), so type errors point
  at 1:1. That is fatal for a language written by agents. LotML must keep spans on every node
  through every pass.
- **DeepWiki was wrong more than once.** It describes plix's never-executed JIT as a working
  tiered JIT, and pycc's docs and DeepWiki agree on components that are absent. Treat either as
  a map only.
- **Gamed benchmarks:** plix special-cased functions named `fib`. Benchmark programs need names
  the compiler cannot see.

## License ledger

| Repo | License | Use |
|---|---|---|
| pon | **none** | ideas only. It also vendors CPython `Lib/` without its LICENSE. |
| interpreter-rs | **none** | ideas only (and it does not build from a clean clone) |
| tarvos | GPL-3.0 (but `pyproject.toml` says MIT) | ideas only |
| pycc | MIT; `tests/corpus/codecontests/` is CC BY 4.0 | copy with attribution |
| spy | MIT; vendored Ryu is Apache-2.0/BSL-1.0 | copy with attribution |
| RustPython, ruff, monty, plix, mamba, depyler | MIT | copy with attribution. `monty-typeshed` lacks typeshed's license, so take stubs from typeshed. depyler ships MIT text only. |
| erg | MIT/Apache-2.0; `doc/` is CC-BY-4.0 | copy with attribution |
| mun | MIT/Apache-2.0 | copy with attribution |
| starlark-rust | Apache-2.0; Go conformance tests are BSD-3 | copy with attribution and NOTICE |
| edge-python | Apache-2.0, per directory; no NOTICE | re-check the directory at copy time |
| lpython | BSD-3; libasr is from LFortran (license not checked) | copy with attribution |

## Caveats about this study

- **Rule breaches:**
  - The pon agent ran `python -` with the clone as its working directory. It waited on empty
    stdin and was stopped, and no repo code ran.
  - The lpython agent read `libasr` from GitHub at the pinned commit, because the submodule is
    empty in the clone. Its `[L]` citations are ±5 lines.
  - The plix agent timed `clang` on LotML's own runtime, writing only to the scratchpad.
- **Missing sources:**
  - `ruff` is a sparse checkout. `ty_python_core` (the semantic index) was not read directly.
  - DeepWiki has no export for edge-python, tarvos or mamba.
- **Unverified finding:** the parser defect (3) comes from reading the code and has not been
  reproduced yet.
