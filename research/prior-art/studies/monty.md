# monty — pydantic's sandboxed Python-subset bytecode interpreter in Rust, on ruff's parser, for LLM-written code

`github.com/pydantic/monty` · MIT (`LICENSE`, "Copyright (c) Pydantic Services Inc. 2026 to present") ·
last commit 2026-10-05 · 196,612 lines of Rust in 18 crates: `monty` 123,743 (105,426 in `src/`),
`monty-proto` 14,444, `monty-pool` 14,383, `monty-fs` 10,509, `monty-types` 8,299, `monty-python` 6,812,
`monty-runtime` 4,203, `monty-js` 3,513 (+ TypeScript), `monty-datatest` 2,998, `monty-type-checking` 1,180;
643 differential test programs in `crates/monty/test_cases/`. Mature and commercially backed (v1.1.0,
PyPI/npm/crates.io, a paid "Full Monty" server outside the repo); it targets CPython 3.14 behaviour on a
deliberately small subset and documents every divergence in 39 `docs/limitations/*.md` pages.

Relevance to LotML: high for *running* LLM code (latency, limits, isolation, type-checking before running),
nil for native codegen — monty has no AOT path.

## Architecture

One pipeline, interpreter only; there is no `build`.

| Stage | Where | Key types |
|---|---|---|
| Parse (ruff) | `crates/monty/src/parse.rs:214-232` calls `ruff_python_parser::parse_module`, then converts ruff's AST | `Parser` `parse.rs:290`, `ParseNode = Node<RawFunctionDef>` `parse.rs:154` |
| Nesting pre-scan | `crates/monty/src/source_nesting.rs:1-25` (one lexer pass bounds parser depth before ruff runs) | `source_within_nesting_bound` |
| Prepare (name resolution) | `crates/monty/src/prepare.rs:52` `prepare_with_existing_names`, state machine `Prepare` `:129` | names to namespace slots, cells/free vars, `PreparedNode` |
| Bytecode compile | `crates/monty/src/bytecode/compiler.rs:252` `Compiler`, `:533` `compile_module`, `:545` `compile_snippet` | `Code` `bytecode/code.rs:14` (bytes, constants, location table, exception table) |
| VM | `crates/monty/src/bytecode/vm/mod.rs:782` `VM`, dispatch loop `run` `:1239` | `CallFrame` `:355`, `FrameExit` `:221` |
| Heap | `crates/monty/src/heap/mod.rs:914` `Heap` over paged `StableHeap` (`heap/stable_heap.rs:11,49`, 256-slot pages) | `HeapEntry` `heap/mod.rs:827`, `HeapData` `heap_data.rs:180` |
| Suspension / snapshot | `crates/monty/src/run_progress.rs:37` `RunProgress`, `:811` `Snapshot {executor, vm_state, heap}` | `FunctionCall`, `OsCall`, `NameLookup`, `ResolveFutures` |
| Host process | `crates/monty-runtime` (`monty` binary; `src/subprocess.rs` worker), `crates/monty-pool` (tokio pool), `crates/monty-proto` (protobuf framing) | `Pool` `monty-pool/src/pool.rs:34` |

The pipeline glue is 25 lines: `compile_module_source` `crates/monty/src/run.rs:851-876` (intern inputs, parse,
prepare, compile, commit interns). Public entry `MontyRun` `run.rs:59` with `run`/`run_no_limits`/`start`
(`:155-195`); REPL `MontyRepl` `repl.rs:51` compiles each feed into an overlay interner
(`intern/compile.rs:15` `CompileInterns`) that is committed only if the snippet is admitted (`CLAUDE.md:250-270`).

`run` vs `build`: not applicable. The only execution surface is a pool of `monty` worker *subprocesses*;
"there is no in-process execution API" (`CLAUDE.md:853`, `:198-200`), because "a monty process can never be
made fully crash-proof against memory errors (stack overflow aborts, allocator aborts)" (`CLAUDE.md:146-148`).

## Frontend

- **Parser is ruff's, from crates.io**: `ruff_python_parser`/`ruff_python_ast`/`ruff_text_size` `0.0.14`, plus
  `ty_python_semantic` `0.0.14` and salsa pinned to "the version ruff uses" (`Cargo.toml:73-88`). Monty never runs
  ruff's semantic checker (`parse.rs:1835`), it converts ruff's `Stmt`/`Expr` into its own `Node<..>` tree and
  rejects unsupported syntax there (class inheritance, `match`, `yield`, `del`, … — `docs/limitations/index.md`).
- **Recursion is Monty's own problem**: "ruff itself no longer limits recursion" (`parse.rs:37`); ruff grows its
  stack with `stacker` outside the sandbox allocator, so a pre-parse lexer scan over-approximates depth for sources
  above a threshold (`source_nesting.rs:1-8`), and the converter enforces `MAX_NESTING_DEPTH` 200 release / 30 debug
  (`parse.rs:40-46`) with a decrementing budget (`decr_depth_remaining` `parse.rs:2236`).
- **Flat `elif` chains are counted as depth**: ruff gives clauses flat, Monty folds them right-nested and charges
  each arm against the depth budget, because prepare and compile recurse over the folded tree
  (`parse.rs:347-359`).
- **Positions ruff's AST loses** are harvested from the token stream before it is dropped: `class` keyword offsets
  for CPython-exact traceback lines (`parse.rs:223-229`, `:300-307`).
- Literal edge: int literals capped by `INT_MAX_STR_DIGITS` (`parse.rs:2640`).
- No incremental parsing; each feed reparses its snippet. Tolerance: first syntax error aborts (ruff's error is
  mapped to `SyntaxError`, `parse.rs:221-222`).

## Semantics and types

- Dynamic Python semantics, executed; no static types at run time. Names are resolved statically to slots in
  `prepare.rs` (locals, cells, globals); an *undefined* name in call position becomes an external call:
  `load_global_callable` pushes an `ExtFunction` for any unbound global that is not a builtin
  (`bytecode/vm/mod.rs:2488-2510`), so host functions need no declaration in the program.
- **Optional static checking of LLM code before running**: `monty-type-checking` embeds ty in-process over an
  in-memory salsa db (`crates/monty-type-checking/src/db.rs`, "Very simple in-memory salsa/ty database").
  Host-provided functions are declared as a stubs file that is prepended as `from <stubs> import *`, diagnostics
  shifted back by the injected offset (`type_check.rs:56-135`). It checks against `monty-typeshed`, typeshed
  *filtered to Monty's runtime surface* (`crates/monty-typeshed/README.md`; whitelists in `update.py` mirror
  `crates/monty/src/builtins/`), zipped into the binary by `build.rs` (adapted from ruff's `ty_vendored`). Rationale:
  "turns a whole class of runtime failures into a diagnostic you can hand straight back to the model as a retry
  prompt" (`docs/type-checking.md`).
- Integers: `Value::Int(i64)` with heap `LongInt` (num-bigint) on overflow (`value.rs:73`, `types/long_int.rs`).
- No generics/monomorphization (interpreter). Unsupported features fail at parse time with `NotImplementedError`
  rather than at run time (`docs/limitations/index.md`).

## IR and passes

- AST → `ParseNode` (`parse.rs`) → `PreparedNode` (`prepare.rs`, names resolved, comprehension scopes, parse-time
  builtin substitution at module scope only, `prepare.rs:1245`) → bytecode `Code`.
- Bytecode: CPython-like stack machine, raw `Vec<u8>`, `#[repr(u8)]` opcodes with explicit discriminants
  (`bytecode/op.rs:70`), operands fetched separately (`op.rs:1-12`: 0, 1, 2-byte and compound operands).
  Specialized forms: `LoadLocal0..3` (`op.rs:96`), `CallBuiltinFunction`/`CallBuiltinType` (`compiler.rs:1778-1806`),
  fused assert comparisons via flag operands (`op.rs:31-58`).
- Zero-cost exceptions: per-`Code` exception table and location table (`bytecode/code.rs:105-223`).
- "Don't bother compiling dead code" (`compiler.rs:631`) is the only dead-code pass; no constant folding pass, no
  inline caches, no quickening (grep finds none). Opcode space is a stated constraint: 256 max, about half used
  (`CLAUDE.md:211-217`).

## Backend and toolchain

- No native backend, JIT or linker use. Builds: native `monty` worker binary (`crates/monty-runtime`), a
  `wasm32-wasip1` component for browsers/Node (`crates/monty-wasm-runtime`, Jco adapter), PyO3 0.29 client
  (`crates/monty-python`, depends on `monty-pool`, *not* on the interpreter), napi client (`crates/monty-js`).
- Host-side crates must not link the interpreter: they depend on `monty-types` only (`CLAUDE.md:30-38`).
- Windows: worker thread runs on a fixed 16 MiB stack because recursion-heavy `repr`/`==` up to the 1000-frame
  limit "fits the 8 MiB main thread of Linux and macOS but not Windows' 1 MiB"
  (`crates/monty-runtime/src/subprocess.rs:29-46`). Virtual paths are POSIX everywhere (`CLAUDE.md:55-71`).
- Release profile: fat LTO, one codegen unit, PGO target (`Cargo.toml` `[profile.release]`, `make dev-py-pgo`).

## Runtime

- **Value**: 16-byte enum, immediates inline (`None`, `Bool`, `Int(i64)`, `Float(f64)`, interned string/bytes/long
  ids, builtin/module-function/def-function ids), heap objects as `Ref(HeapId)` (`value.rs:60-118`); `Clone` is
  deliberately not derived — `clone_with_heap` increments the count (`value.rs:54-56`). `HeapData` is asserted
  ≤ 80 bytes (`heap_data.rs:193`).
- **Memory**: manual reference counting in a paged arena with a free list (`heap/mod.rs:1144` `allocate`,
  `:1303` `inc_ref`, `:1331` `dec_ref`) plus Bacon–Rajan trial-deletion cycle collection (`:1509`
  `collect_cycles`): candidates are entries whose count dropped to non-zero (Purple), so no root enumeration;
  GC runs only when candidates exist and every 100,000 GC-tracked allocations (`:1079-1083`, `should_gc`
  `:1443-1452`). Safe access through branded `HeapReader::with` closures with per-entry reader counts
  (`heap/mod.rs:105-181`, `:827-848`); the `unsafe` lives only in `heap/` (`CLAUDE.md:272-280`).
- **Refcount hygiene** is a discipline, not a type guarantee: `defer_drop!`, `DropGuard`, `drop_with`
  (`CLAUDE.md:337-400`); a `memory-model-checks` feature adds a `Value::Dereferenced` sentinel whose `Drop` panics
  and runs GC on every allocation (`value.rs:111-117`, `crates/monty/Cargo.toml` features).
- **Builtins and stdlib tables**: `Builtins` enum (`builtins/mod.rs:57`) over `BuiltinsFunctions`
  (`crates/monty-types/src/builtins.rs:32`; discriminants are bytecode operands, so reordering bumps the dump
  version, `CLAUDE.md:244-246`); one file per builtin (35 files plus `mod.rs` in `builtins/`); `StandardLib` enum
  (`modules/mod.rs:40`) with ~20 Rust-implemented modules (`math`, `re` on fancy-regex, `json` on jiter, `datetime`,
  `random` Mersenne Twister, `itertools`, `collections`, …). Argument binding is generated by
  `#[derive(FromArgs)]` in `crates/monty-macros/src/from_args.rs` per CPython parser family — hand-written
  parsing was "a known source of reference-count leaks, divergent error messages" (`CLAUDE.md:537-565`).
- **Resource limits** (`crates/monty-types/src/resource.rs:157-182`): per-feed and per-turn duration, memory,
  GC interval, recursion depth (default 1000, `:185`), host-suspension count (default 1000, `:190`), total sleep.
  The VM runs a checkpoint every 255 instructions (`bytecode/vm/mod.rs:1250`), measured at "~40% on tight loops
  at 10, ~2% at u8::MAX" (`:1240-1246`); with no limits armed the check is one hoisted branch (`:1252-1255`).
  Memory is *soft* (graceful `MemoryError`) via allocator-backed counts; the hard ceiling is a global allocator,
  `crates/monty-alloc/src/lib.rs`, that charges every `alloc`/`realloc` and exits the process with an OOM code past
  the ceiling (`:47-118`). Amplifying string builders must go through `StringBuilder` (`string_builder.rs`,
  `CLAUDE.md:402-423`).
- **External-call control**: execution returns `RunProgress::{FunctionCall, OsCall, NameLookup, ResolveFutures,
  Complete}` (`run_progress.rs:37-49`); the host answers with `resume`/`resume_pending` (async futures)/`abort`
  (`:134-216`). OS access is never performed by the interpreter: file, env, clock calls suspend as `OsCall`, and
  `OsPolicy` (`crates/monty-types/src/os_policy.rs:21-33`) chooses system/host/fixed clock, zone, sleep mode,
  `process_time`, random seed — deterministic runs are a configuration. Filesystem mounts are host-side, confined
  structurally through a `cap_std::fs::Dir` handle (`CLAUDE.md:112-142`).
- **Snapshot/resume**: a suspended `Snapshot` owns compiled code, interns, VM state and the whole heap
  (`run_progress.rs:811-819`); serialized as CBOR via serde with field names (single upper-case letters on hot
  types) as the compatibility contract (`dump_format.rs`, `CLAUDE.md:231-248`), `DUMP_VERSION` 13 with
  `MIN_SUPPORTED_DUMP_VERSION = DUMP_VERSION` (`dump_format.rs:52-55`). Trust model: snapshots are trusted input;
  invalid ones may panic but must not cause UB (`CLAUDE.md:97-110`).
- **Isolation**: worker processes, one framed protobuf request in, streamed `Print` events, exactly one
  turn-ending event; a worker that exits without a `FatalError` frame is replaced; tokio turn deadlines
  (`CLAUDE.md:144-205`). Values cross as one post-order arena per message so shared sub-objects cross once
  (`crates/monty-proto/proto/monty/v1/monty.proto:44-57`). Latency: pool checkout plus `1 + 1` 0.80 ms, ten REPL
  feeds 0.40 ms (`docs/index.md:34-41`).
- **CPython interop**: none at run time (the point is to not have CPython); the Python package is a client that
  talks to workers.

## Testing and conformance

- **Differential against CPython in one process**: `monty-datatest` runs each of the 643 `test_cases/*.py` on Monty
  and on CPython through pyo3 (`crates/monty-datatest/src/main.rs:1-36`, `:2393`), with a 30 s CPython watchdog
  (`:2233-2305`). Expectation formats: plain `assert`s (preferred), `"""TRACEBACK:..."""` compared frame by frame
  with carets normalised, `# ref-counts={...}` (62 `refcount__*` files), markers `# call-external`, `# run-async`,
  `# timezone=`. Rule: "NEVER MARK TESTS AS XFAIL" (`CLAUDE.md:796-819`).
- Rust integration tests per concern (`crates/monty/tests/`: `resource_limits.rs` 70 KB, `repl.rs` 81 KB,
  `dump_compat.rs` loads a checked-in dump fixture, `heap_reader_compile_fail.rs` asserts unsafe-API misuse does not
  compile). `insta` snapshots for multi-line output.
- Fuzzing: `crates/fuzz/fuzz_targets/string_input_panic.rs` and a structured `tokens_input_panic.rs` (Arbitrary
  token enum generating plausible Python, `:1-40`).
- Wire protocol: `monty-proto/tests/differential.rs` proves the hand-written encoder byte-identical to a
  prost-generated oracle.
- Docs are executed: every Python and TypeScript snippet in `docs/` runs in CI (`CLAUDE.md:1031-1075`).
- Benchmarks vs CPython via pyo3 locally, CodSpeed (instruction counting) in CI
  (`crates/monty-bench/benches/main.rs:14-16`, `.github/workflows/codspeed.yml`).

## Reusable for LotML

| Item | Path in monty | What it gives | LotML crate | License verdict | Effort |
|---|---|---|---|---|---|
| Rust `.pyi` reading for `lotml bind` | `Cargo.toml:73-78` (ruff crates on crates.io), `crates/monty/src/parse.rs:214-231` (how to call and convert) | parse typeshed stubs without spawning Python (today `lotml bind` runs `lotml-py/runtime/lotml_bind.py` with `ast`, `lotml/src/exec.rs:449-478`) | `lotml` (`exec::bind`), new module beside `lotml-py/src/boundary.rs` | ruff crates MIT: depend; monty glue: idea only | M |
| Vendored, zipped typeshed in the binary | `crates/monty-typeshed/build.rs`, `src/lib.rs`, `update.py` | `lotml bind` without an installed mypy/jedi for the stub (current fallback, `lotml_bind.py:1-6`) | `lotml` | build.rs MIT (from ruff, MIT) adapt; the stubs themselves are typeshed's (Apache-2.0/MIT), not monty's MIT label | S |
| Counting global allocator with hard ceiling | `crates/monty-alloc/src/lib.rs:1-150` | live-byte count plus deterministic exit code on overrun | idea for `lotml-runtime/c/lotml.c` (`lt_malloc` `:202`) — C, so a port, not a copy | MIT, adapt (port to C) | S |
| Dispatch checkpoint cadence and cost data | `crates/monty/src/bytecode/vm/mod.rs:1239-1273` | evidence for amortising a limit check: decrement-and-branch per step, outlined slow path, 255 steps | `lotml-ir` (a fuel statement) / `lotml-llvm/src/emit.rs` | idea | S |
| Worker pool protocol shape | `CLAUDE.md:144-205`, `crates/monty-pool/src/pool.rs:34-109`, `crates/monty-runtime/src/subprocess.rs` | strict-alternation framed stdin/stdout protocol, crash = EOF without fatal frame, replace worker, per-turn deadline | `lotml` (`exec.rs` `wait_limited` `:148-185`, `mcp.rs` test tool) | MIT; tokio/protobuf machinery is heavier than LotML needs: idea | M |
| Differential harness conventions | `crates/monty-datatest/src/main.rs`, `CLAUDE.md:633-830` | one-process differential run, watchdog per case, traceback-exact expectations, ref-count expectation lines, no xfail | parity suite (`lotml-py` vs `lotml-llvm`) | idea | S |
| Structured-token fuzzer | `crates/fuzz/fuzz_targets/tokens_input_panic.rs` | grammar-shaped fuzz input instead of bytes | `lotml-syntax`, `lotml-check` | MIT, adapt the token enum to LotML syntax | S |
| Type-check-before-run for LLM code | `crates/monty-type-checking/src/type_check.rs:56-135` | confirms LotML's design (check, then hand diagnostics back); nothing to copy, LotML has its own checker | — | nothing | — |

Nothing in the interpreter core (`bytecode/`, `heap/`, `types/`) is reusable: it implements dynamic Python.

## Ideas and optimizations worth adopting

Ranked by expected impact on LotML.

1. **A warm CPython worker pool behind `lotml run`/`lotml test`, the MCP `test` tool and the grader.**
   What: keep N Python processes with `lotml_rt` imported; send a compiled module over a framed stdin protocol,
   stream prints, end each turn with one result frame; replace a worker that dies without a result frame or runs
   past the deadline (monty: `CLAUDE.md:144-205`). Why: monty's 0.8 ms vs seconds comes from the pool, not the
   interpreter (`docs/index.md:34-41`, "a sandbox is a checkout from a pool of worker subprocesses"); LotML pays a
   CPython start plus `import lotml_rt` per run (`lotml/src/exec.rs:215-235`), and verification cost dominates RL
   rollouts ("code is slow to verify", `docs/wiki/pages/rl-environment.md:77-78`). ADRs: fits
   adr:0025 (still CPython for `run`). Risk: state leaking between runs in one interpreter — load each module
   fresh, recycle a worker after any failure or every K runs.
2. **One resource model for both targets: fuel, recursion depth, memory.** What: (a) a fixed maximum call depth,
   checked in native function prologues and set as `sys.setrecursionlimit` in `lotml_rt`, reported as the same panic;
   (b) a fuel counter charged at loop back-edges and calls (edge-python's metering points, see its study) emitted
   from the IR so both backends meter alike, with monty's cadence (cheap decrement, outlined check);
   (c) byte counting in `lt_malloc` (route the bare `malloc`/`realloc`/`calloc` calls in
   `lotml-runtime/c/lotml_list.c:239`, `lotml_dict.c:219,348,474,514` through it) with an env-var ceiling, monty-alloc
   style. Why: today the Python target fails at CPython's default 1000 frames (`lotml_rt.py` sets no limit; grep
   finds no `setrecursionlimit`), while native code fails at the OS stack — 1 MiB on Windows, the gap monty
   documents (`monty-runtime/src/subprocess.rs:29-34`) — so the two targets disagree on recursion depth, and the
   driver sets no stack size (`lotml-llvm/src/driver.rs`). The harness meters Python by `sys.monitoring` LINE events
   (`harness/lotml_harness/execute.py:134-160`), which native code cannot reproduce. ADRs: adr:0020 (lower once in
   the IR — the right place), adr:0025 (parity is defined against the Python target); a resource-exhausted panic is
   new language behaviour and needs a spec — **no conflict, but it is a language change, not a tooling change**.
3. **`lotml bind` without Python.** What: read stubs with `ruff_python_parser` and ship a zipped, vendored typeshed
   (monty-typeshed). Why: `bind` today needs a Python interpreter and, without `--stub`, an installed mypy or jedi
   for typeshed (`lotml_bind.py:1-6`); the compiler already has salsa 0.28 like monty, but the parser crate alone has
   no salsa dependency. ADR: **adr:0012 says `lotml bind` "reads a stub with Python's own parser"** — this changes
   that sentence and needs an ADR amendment; it does not touch adr:0012's rejection of reading `.pyi` at check time
   (bind stays a separate, explicit step). Cost: ruff crates are `0.0.x`, so every bump can break the API.
4. **Count `elif` arms against the depth guard, or lower `elif` iteratively.** What: monty charges each folded
   `elif` against `MAX_NESTING_DEPTH` (`parse.rs:347-359`). LotML parses `elif` in a loop
   (`lotml-syntax/src/parser.rs:786-825`) so `MAX_DEPTH` (`:87`) never sees it, then `if_chain`
   (`lotml-ir/src/lower.rs:1636-1649`) recurses once per arm and builds a right-nested `If` that every IR pass
   recurses into. Mitigated today by the 256 MiB compiler thread (`lotml/src/main.rs:227-235`); a very long
   generated elif chain is the residual risk. No ADR touched. Effort S.
5. **In-process interpreter for `lotml run` — what it would cost (not recommended now).** Monty needs 105k lines
   of `src/` for a Python subset (dynamic dispatch, CPython-exact errors, ~20 stdlib modules, refcount discipline,
   snapshotting). A LotML interpreter would be far smaller: it would walk the *monomorphic, counted* IR
   (`lotml-ir/src/ir.rs:276-380`: ~15 statement kinds, typed locals, `Builtin` table of ~120 operations
   `ir.rs:13-260`) with types already known, no attribute lookup, no classes. Estimate 5–8k lines plus a third
   implementation of every builtin beside `lotml_rt.py` (1,086 lines) and the C runtime (~4,900 lines). Gains:
   sub-millisecond runs, deterministic fuel, snapshot/resume (monty's `RunProgress` model maps onto IR statements),
   no CPython for programs without Python imports. Losses: Python imports cannot run in-process — they would need
   monty-style suspension (`FunctionCall`) to a CPython worker, which is idea 1 again. **Conflicts with
   adr:0025** (two targets; it rejects a third mechanism for what `run` gives) **and with the reasoning of
   adr:0020/adr:0021** (every emitter is one more consumer of every construct). Idea 1 plus 2 capture most of the
   latency and determinism without it; revisit only if pool latency is still the RL bottleneck.
6. **Run-environment policy as data (`OsPolicy`).** Clock, zone, sleep, `process_time`, random seed fixed per run
   (`monty-types/src/os_policy.rs:21-33`). LotML exposes no I/O beyond `print` and `math` today
   (`docs/wiki/pages/rl-environment.md:86-89`); keep this pattern for when a clock or randomness enters the prelude,
   so graded runs stay reproducible. No ADR touched.
7. **Live-count expectations in parity tests.** Monty asserts reference counts at program end
   (`# ref-counts={...}`, 62 files). LotML already prints live cells at exit (`lotml-runtime/c/lotml.c:68-77,182`);
   make a non-zero count a parity-suite failure if it is not already. adr:0008 (reuse) benefits. Effort S.

## Pitfalls seen

- **In-process sandboxing was given up.** Despite a memory-safe Rust interpreter, stack overflow and allocator
  aborts kill the host, so all Python and JS execution goes through worker subprocesses (`CLAUDE.md:146-148`,
  `:853`). Lesson for LotML: any in-process runner (idea 5) would still need process isolation for graded code.
- **Hand-written reference counting in Rust leaks.** Three cleanup mechanisms with ranked preference, a
  "very slow" CI-only `memory-model-checks` build, 62 ref-count test files, and the `FromArgs` derive introduced
  because hand-written argument parsing leaked counts (`CLAUDE.md:337-400`, `:537-565`, `:657-664`). LotML's
  choice to insert counts in the IR (`lotml-ir/src/own.rs`) rather than by hand avoids this for compiled code; the
  C runtime remains hand-counted and is where the same bugs would appear.
- **Soft limits are polled, so single builtins overshoot.** `str.expandtabs` with a huge tab size allocated
  gigabytes between checkpoints; `*args`, JSON arrays and `re.findall` each "killed the worker on ordinary code"
  until preflighted; `re.finditer` is "still open" (`CLAUDE.md:402-463`). Any LotML fuel/memory scheme must charge
  inside amplifying runtime functions (`str * n`, `join`, list repeat), not only at loop back-edges.
- **A resource error leaves the heap inconsistent** ("orphaned objects with incorrect refcounts"), so the session
  must be discarded (`CLAUDE.md:944-946`). Treat resource exhaustion as terminal in LotML too.
- **Dump compatibility is designed but not delivered**: name-keyed CBOR and alias rules, yet
  `MIN_SUPPORTED_DUMP_VERSION = DUMP_VERSION` (`dump_format.rs:52-55`), so every bump invalidates stored snapshots.
- **Dependency on pre-1.0 ruff/ty crates pins salsa** to ruff's version (`Cargo.toml:84-88`); adopting ty itself
  would bind LotML's salsa (`compiler/Cargo.toml:12`, 0.28.5) to Astral's release cadence. The parser crate alone
  avoids this.
- **Opcode budget**: one-byte opcodes, about half of 256 used, flags operands preferred over new opcodes
  (`CLAUDE.md:211-217`).
- **Documentation surface cost**: four hand-synchronised doc surfaces, named duplication points, a docs-parity
  review agent (`CLAUDE.md:970-1080`); 39 limitations pages exist because the subset is defined by divergence from
  CPython. LotML defines its own semantics, so it should keep one reference (`reference/lotml.md`) rather than a
  divergence catalogue.
- **License note**: `crates/monty-typeshed` is labelled MIT (workspace license) but vendors typeshed stubs without
  typeshed's LICENSE file (`find crates/monty-typeshed -iname '*licen*'` finds none); take stubs from upstream
  typeshed under its own license.
