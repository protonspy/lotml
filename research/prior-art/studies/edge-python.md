# edge-python — single-pass bytecode compiler and tiered register VM for a sandboxed dynamic Python subset, shipped as one WebAssembly module

`github.com/dylan-sutton-chavez/edge-python` · Apache-2.0 (`LICENSE.md`, by directory; `Cargo.toml`
`[workspace.package] license`) · last commit 2026-10-07 · engine `src/` 25,221 lines of Rust (parser 4,145,
VM 13,616 incl. `vm/dispatch.rs` 1,628, values/heap 1,697, lexer 832, optimizer 277, modules 2,113, wasm
glue 1,287); `cli/` 9,221 Rust (wasmtime + SpiderMonkey host), `js/` 3,135 TS, `pdk/`+`abi/` 1,296; tests:
5,496 golden VM cases in `tests/cases/vm.json`. No DeepWiki. Young but very active single-author project
(v0.9.4), heavily documented in `docs/` (MDX) and `skill/SKILL.md` for LLMs; dependencies are only
`hashbrown`, `itoa`, `libm` (+ `dlmalloc` on wasm).

Relevance to LotML: low for the compiler (it is a dynamic-language interpreter; its "SSA" does not survive to
execution), medium for running and grading programs (deterministic metering, memory model, deterministic
benchmarks).

## Architecture

"The compiler is a single pass with no AST and no IR. Bytecode is the only intermediate representation"
(`docs/05-internals/01-design.mdx`). Four stages, all in the root crate `compiler` (`src/lib.rs:1-31`):

| Stage | Where | Key types |
|---|---|---|
| LUT lexer | `src/lexer/scan.rs`, tables `src/lexer/tables.rs:10` `BYTE_CLASS`, `:28` `SINGLE_TOK`, `:60` `keyword` | `Token {kind, line, start, end}` (offsets, no text copies) |
| Pratt parser emitting bytecode | `src/parser/mod.rs:52-81` `Parser`, `src/parser/{expr,stmt,control,literals,imports}.rs` | `SSAChunk` `src/parser/types.rs:168`, `Instruction {opcode: u8, operand: u16}` |
| Post-parse finalize + optimizer | `parser/types.rs:256-309` `finalize_prev_slots`; `src/optimizer.rs:6-64` `constant_fold` | `phi_sources`, `phi_map` |
| Tiered VM | `src/vm/mod.rs`, `src/vm/dispatch.rs`, register lowering `src/vm/lower.rs:101` | `VM`, `Code`/`Ins {op, x: u8, a, b, c: u16}` `lower.rs:9-15` |

Compile entry used by the wasm host: lex, parse with an injected import `Resolver`, render diagnostics, fold
(`src/wasm/exports.rs:35-54`), with a source-keyed chunk cache (`:57-78`).

`run` vs `build`: there is no native code generation. `edge build` appends the project bundle and a trailer
(`EDGESFX\x01`) to the CLI's own executable (`cli/src/cmd/build.rs:16-36`); the CLI runs a precompiled
`compiler.wasm` under wasmtime (`cli/Cargo.toml`). Both commands share the one interpreter.

## Frontend

- **Lexer**: byte-class LUT plus two-load single-char operator dispatch and keyword lookup routed by
  `(length, first byte)` (`lexer/tables.rs:10-60`); comments kept as tokens for round-tripping; soft keywords
  (`match`, `case`, `type`) decided by a post-scan pass; indentation by a column stack frozen inside brackets;
  mixed tabs/spaces halts with `Endmarker` (`docs/05-internals/02-compiler.mdx`, "Indentation").
- **Parser**: Pratt `expr_bp` with per-operator binding powers; each construct is parsed and *emitted* in the same
  traversal. Error handling: diagnostics accumulate (`parser/mod.rs:76`), an unclosed-bracket stack drops cascade
  errors (`:74-75`), and on any error the bytecode is cleared before finalize (`parser/mod.rs:560-576`).
- **No AST, so constructs that need lookahead are patched after the fact**: a conditional expression drains the
  already-emitted value and condition and re-emits them in the other order, shifting jump targets
  (`parser/expr.rs:27-41`, `push_shifted` `:59`); an assignment target is recovered by *reinterpreting emitted
  load instructions* via a stack-effect table (`parser/stmt.rs:402` `stack_delta`, `:416` `split_display`, `:435`
  `range_target`), which fails when a `Phi` is inside the target ("Phi cannot move", `:438-439`).
- Limits: `MAX_EXPR_DEPTH` 200 (`parser/expr.rs:75`), 65,535 instructions/names/constants per chunk via an
  `overflow` flag (`parser/types.rs:8`, `:212-252`). Imports resolve at parse time through the host resolver; no
  import opcode reaches the VM (`parser/mod.rs:77-80`, `parser/imports.rs`).
- No incremental/salsa machinery; the wasm host caches whole parsed chunks by source text (`wasm/exports.rs:57-78`).

## Semantics and types

- Dynamic Python subset: classes with C3 MRO, async/await, `match`, generators as coroutines, imports resolved at
  compile time. Annotations are parsed and dropped (`docs/05-internals/02-compiler.mdx`, "Functions").
- **Deliberate divergences for speed or size**: ints are 48-bit inline, promoted to `LongInt(i128)`, `OverflowError`
  past ±2¹²⁷ (`src/value/mod.rs:156-162`; design doc "Memory model"); generator expressions are materialised eagerly
  to lists so memoisation sees finite arguments; no `complex`, `Decimal`, `bytearray`, no `gen.send`.
- **Scopes are static**: names resolve once to frame slots, cells, or module binding indices
  (`src/vm/scope.rs:10-40` `Globals`), builtins answer while a name is unbound.
- **Purity** for memoisation: static flag set when a function body contains no store-item/attr, print/input,
  global/nonlocal, raise or yield opcodes (`parser/literals.rs:741`), plus run-time propagation through calls and a
  check that every free name is "bound once" — computed from the SSA version index (`vm/opcodes/function.rs:248-262`).
- No generics/monomorphization; no static types at all.

## IR and passes

The bytecode is the IR. What happens to it, in order:

- **Name versioning ("SSA")** — every store bumps a per-name version and stores into a slot named `x_<n>`
  (`parser/mod.rs:86-147`); a read uses the current version; a name read before binding is `x_0`. Control-flow
  joins push a `JoinNode {backup, then}` snapshot of the whole version map (`parser/mod.rs:212-229`) and
  `commit_block` emits a `Phi` for each name whose version differs between branches (`:231-264`), sources kept out of
  line in `phi_sources`, indexed by `phi_map` (`parser/types.rs:326-331`).
  **Loops get no header phis**: `while` reads the condition at the pre-loop version and the body stores new versions
  (`parser/control.rs:413-440`, `for` `:444-479`).
- **Version coalescing undoes it** — `finalize_prev_slots` links each `x_n` to `x_{n-1}` and rewrites every
  `LoadName`/`StoreName`/`Del`/`Phi` operand and every phi source to the chain's root slot
  (`parser/types.rs:256-309`). After this, all versions of a name share one slot; that is why loops are correct.
- **Constant folding** on stack code: `LoadConst LoadConst <binop>` and unary `Not`/`Minus` folded with the run-time
  rules (floored modulo, `-0.0` kept apart from `0.0` in the constant pool) (`optimizer.rs:16-33`, `:133`, `:228-260`).
- **Phi-noop elimination** — after coalescing, a phi whose sources and target are the same slot is deleted; dead
  instructions compacted with jump remapping (`optimizer.rs:35-56`, `:67-105`). The doc lists what it deliberately
  does not do: no SSA-wide constant propagation, CSE, GVN, LICM, inlining, branch DCE (`01-design.mdx`,
  "What the compiler intentionally does not do").
- **Run-time tier-up** (`src/vm/registers.rs:20-38`): a chunk's first call runs the compiler's stack code (with
  `LoadAttr`+`Call` fused into `CallMethod`, `lower.rs:643`); its second call, or 16 back-edges in one frame
  (`lower.rs:51` `HOT_LOOP`, `dispatch.rs:1046`), lowers it once to register code shared by all frames.
- **Stack→register lowering** (`vm/lower.rs:101-157`): abstract interpretation of the operand stack keeps up to 16
  pending values as registers (`:85-98`); frame layout is names, then constants, then None/True/False, then
  temporaries (`:116-117`); binary ops get register forms (`:597-611`), compare-feeding-branch becomes
  compare-and-jump (`:616-623`); a definite-binding bitset dataflow over blocks (`bound_on_entry` `:184-240`) lets a
  load skip its unbound check. Falls back to stack code if the frame exceeds `u16` registers.

## Backend and toolchain

- Targets: `wasm32-unknown-unknown` `cdylib` (`compiler.wasm`, 722 KB raw, `opt-level = "z"`, LTO, `panic = abort`,
  then `wasm-opt`), and an rlib for native embedding (`Cargo.toml` `[lib]`, `[profile.release]`,
  `[profile.cli]` at `opt-level = 3` for the CLI's precompiled copy).
- No JIT, by decision: "A method JIT needs stencils per architecture, and a tracing JIT duplicates the execution
  model and complicates the GC" (`01-design.mdx`).
- Host ABI: values cross as NaN-boxed `u64` with tags published in the `abi` crate (`abi/src/lib.rs:12-19`); plugins
  are `.wasm` modules built with `pdk/` and resolved at compile time; host functions are `ExternFn {name, func,
  pure}` (`src/value/mod.rs:86-95`). Windows: CLI install targets macOS/Linux/WSL (README); `make` works on Windows
  (`CONTRIBUTING.md:26`).

## Runtime

- **Value**: 8-byte NaN-box `Val(u64)` (`src/value/mod.rs:111`): non-canonical floats as themselves, 48-bit ints,
  None/True/False/undef immediates, 28-bit heap index (`abi/src/lib.rs:12-19`, predicates `value/mod.rs:170-178`).
  Each new NaN gets a fresh id so `is` distinguishes NaNs (`:139-150`). Equality unifies `1 == 1.0 == True` for dict
  keys (`:113-128`).
- **Int fast path**: with 48-bit payloads, `a + b` in `i64` cannot overflow, so add/sub are a plain add plus a range
  check (`src/vm/registers.rs:187-189`); register arithmetic tests both tags inline and falls back to the generic
  handler (`vm/dispatch.rs:382-401`).
- **Heap**: `Vec<HeapSlot>` arena with free list capped at 524,288 entries (`value/mod.rs:667-760`, `:957-959`);
  `HeapObj` variants for every type (`:199`); interning of short strings, bytes ≤ 128 B, all `LongInt`s, types,
  bound methods (`:696-709`). Containers are `Rc<RefCell<…>>` inside the arena.
- **GC**: non-moving mark–sweep, no reference counts; triggers on `live >= gc_threshold` or allocation count
  (`value/mod.rs:1003-1004`); thresholds rescale from the last mark (`:951-952`). Roots enumerated explicitly
  (stack, frame slots, module tables, inline-cache values, memo entries; design doc "Garbage collection").
- **Inline caches**: per-instruction `Site` (field index, method + class epoch, builtin method by variant, module
  attribute, operator dunder) (`src/vm/sites.rs:11-29`), stored in a per-chunk `CachePool` shared across frames,
  one extra cache per recursion depth (`src/vm/cache.rs:8-47`).
- **Template memoisation**: pure user functions cache `args -> result`, max 256 entries, disabled after 256 misses in
  a row (`vm/cache.rs:145-170`; call path `vm/opcodes/function.rs:349-363`, keep rule `:267-278`).
- **Limits** (`src/value/mod.rs:19-37`): `Limits {ops: 100_000_000, memory: 256 MiB}`, call depth fixed at 256
  "below where either host runs out of wasm stack" (`:26`). The **op budget is charged only at taken branches and
  back-edges** (`vm/dispatch.rs:403-410`, register `ForIter` `:513-533`), at each call (`vm/opcodes/function.rs:327`)
  and for O(n) native work (`vm/dispatch.rs:1413-1425`, repr rendering `vm/opcodes/dunder.rs:312`). **Memory is a
  byte model, not the allocator**: 224 B per object, 8 B per held value, 56 B per dict entry, 24 B per set entry,
  string length — "the same on every architecture" (`value/mod.rs:28-37`,
  `docs/04-reference/05-limits-and-errors.mdx:8-40`); `'x' * 10**10` raises before allocating.
- **Snapshots**: magic + format 7 + program fingerprint (`src/vm/snapshot.rs:13-14`, `:464`, `:626-629`);
  `Val` bits written verbatim so heap indices, cycles and interning survive; code is not stored but re-parsed from
  the embedded source; caches start cold (design doc "Snapshots"). Preemption every *n* back-edges yields a
  snapshot point even for programs that never suspend.
- No CPython interop.

## Testing and conformance

- Golden expected output, not a CPython oracle: 5,496 `{"src", "output"}` cases in `tests/cases/vm.json` run under
  `Limits::sandbox()` and again in strict-input mode (`tests/vm.rs:98-160`), duplicate cases rejected (`:89`);
  lexer/parser/modules/snapshot/abi JSON suites beside it.
- `tests/memory.rs:1-60`: a counting global allocator checks that the byte model "never counts less than what a
  program holds" (`:59`), and a `memcheck` feature recounts every slot at each GC (`Cargo.toml` features).
- AFL fuzzer drives lexer → parser → VM with a tight op budget and preemption, snapshotting and restoring the first
  park (`fuzz/src/main.rs:14-60`); run daily for 20 minutes, and Miri runs daily per test suite
  (`docs/05-internals/03-runbook.mdx:12-13`, `:31`).
- `coverage` feature records each executed instruction for branch-level coverage tests (`tests/coverage.rs`).
- **Deterministic benchmarks**: `bench/src/main.rs` runs `compiler.wasm` under wasmtime with fuel enabled
  (`:57`, `:180`) and compares wasm-instruction counts and memory peaks to a checked-in `.snapshot`, gating the
  geometric mean at ±0.5% and each case at ±5% (`:66`, `:112-146`) — "the same on every machine and every run".

## Reusable for LotML

| Item | Path in edge-python | What it gives | LotML crate | License verdict | Effort |
|---|---|---|---|---|---|
| Metering points (taken back-edge, call, O(n) builtin) | `src/vm/dispatch.rs:403-410`, `:1413-1425`; `src/vm/opcodes/function.rs:327` | where a fuel counter must be charged to bound every program, and nowhere else | `lotml-ir` (a fuel statement at loop heads and function entry), `lotml-llvm/src/emit.rs`, `lotml-py/src/from_ir.rs`, `lotml-runtime/c/lotml_text.c` / `lotml_list.c` for amplifying ops | idea | M |
| Byte-model memory accounting + "model never undercounts" test | `src/value/mod.rs:28-84`, `tests/memory.rs:1-60` | architecture-independent memory limit and a test proving it is conservative | `lotml-runtime` (`lt_malloc`), harness | Apache-2.0: adapt the test with attribution and the license text | S |
| Deterministic instruction-count benchmark gate | `bench/src/main.rs:14-146` | noise-free CI gate (geomean band + per-case band) | `harness/lotml_harness/experiments/benchmarks.py` (today `min` of wall-clock runs, `:80-81`) | idea (wasmtime-specific; for native code use an instruction counter such as cachegrind or `perf stat`) | M |
| Constant-fold edge cases | `src/optimizer.rs:133`, `:228-260` | `-0.0` must not share a constant with `0.0`; fold only with the run-time rules | `lotml-ir` if a fold pass is added (none today besides set displays, `lower.rs:2140`) | idea | S |
| Structured fuzz harness incl. snapshot round trip | `fuzz/src/main.rs` | fuzz the whole pipeline under a budget | `lotml-syntax`→`lotml-ir` | Apache-2.0, adapt | S |

Nothing else transfers: the lexer tables, Pratt-emitter, register VM, NaN-boxing, inline caches, memoisation and
mark–sweep heap all serve a dynamically typed interpreter.

## Ideas and optimizations worth adopting

Ranked by expected impact on LotML.

1. **Meter only at back-edges and calls, decided in the IR.** What: a `Fuel` charge at each loop head (or
   back-edge) and function entry, plus a size-proportional charge inside amplifying runtime functions; exhaustion
   is a dedicated panic. Why: edge-python bounds every program this way with one decrement per iteration
   (`vm/dispatch.rs:403-410`), while the LotML harness pays a Python callback per *line* through `sys.monitoring`
   (`harness/lotml_harness/execute.py:134-160`) and native programs have no metering at all; an IR-level charge
   gives both targets the same budget, deterministic across machines, so a timeout is a property of the program,
   not of the grader's load (the RL wiki's timeouts are wall-clock, 3× the reference runtime,
   `docs/wiki/pages/rl-environment.md:79-83`). A cheaper interim step in the harness alone: listen to
   `JUMP`/`BRANCH` and `PY_START` events instead of `LINE`. ADRs: adr:0020 (one lowering for both backends is
   exactly where it belongs), adr:0025 (parity); new panic kind needs a spec — **no conflict, but a language-level
   change**. Pair with monty's checkpoint cadence (see its study).
2. **A deterministic memory model for graded runs.** Count requested bytes in `lt_malloc` (and route the direct
   `realloc`/`calloc` calls in `lotml-runtime/c/lotml_list.c:239`, `lotml_dict.c:219,348,474,514` through it) and,
   in `lotml_rt`, charge list/str/dict growth by a fixed model; fail with the same panic when a configured ceiling is
   crossed, and keep a test that the model never undercounts real allocation (`tests/memory.rs:59`). Why: the
   harness caps memory with an rlimit or a job object (`harness/lotml_harness/confine.py:1-18`), which is
   OS-dependent and ends the process instead of reporting. No ADR conflict (adr:0003/0008 untouched).
3. **Instruction-count benchmark gating.** LotML's benchmark gate holds numeric programs to 2× C on the minimum of
   wall-clock runs (`harness/lotml_harness/experiments/benchmarks.py:1-25`, `:80-81`). Edge-python gates on
   counted instructions with ±0.5% geomean and ±5% per case (`bench/src/main.rs:66`); monty uses CodSpeed for the
   same reason. A counted-instruction column in CI would catch codegen regressions (count insertion, reuse) that
   timer noise hides. Keep the wall-clock 2× C target as the headline. No ADR touched.
4. **Fixed call-depth limit shared by every host.** `MAX_CALLS = 256` sits below the smallest host stack
   (`value/mod.rs:25-26`). LotML's two targets fail recursion at different depths (CPython's default 1000 frames
   vs the native OS stack; see the monty study, idea 2). One language-defined depth limit, enforced in both
   backends, removes the divergence. adr:0025 (parity).
5. **Constant folding, if LotML adds it, must replay run-time rules exactly** — floored `%` and `//` signs, `-0.0`
   (`optimizer.rs:133`, `:249`), and for LotML the overflow trap (a fold that overflows must become the same panic
   or a compile error, never a wrapped constant). adr:0007 (`/` on integers returns `f64`) also applies to folds.
6. **Definite-binding dataflow as bitsets per block** (`vm/lower.rs:184-240`). LotML's checker already rejects
   use-before-assignment statically, so this is only a reference implementation if the IR ever needs the fact
   after lowering (e.g., to drop a defensive initialisation). Low.

What does **not** transfer, stated so it is not re-proposed:

- **Single-pass SSA construction.** It is name versioning (`x_1`, `x_2` strings in the name table) with phis only
  at `if`/`try` joins; loops get none (`parser/control.rs:413-440`) and `finalize_prev_slots` coalesces every version
  back into one slot before execution (`parser/types.rs:256-309`), after which the optimizer deletes the self-copy
  phis (`optimizer.rs:35-56`). The only lasting product is the "bound once" fact used by memoisation
  (`vm/opcodes/function.rs:248`). LotML's IR is structured with named locals by decision (adr:0020: basic-block CFG
  rejected as the shared form), and the LLVM emitter uses entry-block `alloca`s promoted by `mem2reg` at `-O2`
  (`lotml-llvm/src/emit.rs:1-2`, `:499-528`), which is the standard and sufficient route to SSA. If LotML ever
  needs SSA for its own passes, the reference is Braun et al. (on-the-fly, with incomplete loop phis sealed later),
  not this.
- **NaN-boxing, inline caches, register lowering, superinstructions**: all exist to recover static facts a
  dynamic language lacks; LotML's monomorphic IR already has the types, and LLVM allocates registers.
- **Automatic memoisation**: semantically safe only for pure, terminating functions and it changes space and time
  behaviour invisibly; LotML's value semantics (adr:0003/0008) would make the purity analysis easy, but a hidden
  cache contradicts predictable performance. Not recommended.

## Pitfalls seen

- **No AST costs flexibility**: ternaries are drained and re-emitted (`parser/expr.rs:27-41`), assignment targets
  are reverse-engineered from emitted bytecode with a stack-effect table (`parser/stmt.rs:402-460`), and a target
  containing a phi cannot be relocated (`:438-439`). LotML's tolerant parser plus AST plus IR (adr:0009, adr:0020)
  avoids this class of patching.
- **"SSA" without loop phis is only correct because it is undone**; any optimization that trusted the versions
  (constant propagation through `LoadName`) would miscompile loops — the design doc rules it out explicitly
  ("No SSA-wide constant propagation through `LoadName`").
- **Version maps are cloned at every block** (`parser/mod.rs:212-233`: `backup`, `then`, `post` are full
  `HashMap<String, u32>` clones), O(names × blocks) per function; SSA names are strings parsed back with
  `rfind('_')` (`parser/types.rs:359-373`).
- **`u16` everywhere**: 65,535 instructions, names, constants per chunk and `u16` registers; lowering silently falls
  back to stack code when a frame does not fit (`vm/lower.rs:146-153`).
- **Semantic shortcuts become user-visible divergences**: 48-bit inline ints with an i128 ceiling, eager generator
  expressions, merged numeric dict keys (`1`, `1.0`, `True` share one slot, `value/mod.rs:113-128`). LotML's
  fixed-width integers with a trap (adr:0007 and the overflow rule) are the explicit version of the same trade.
- **`edge build` is not a compiler**: it appends the bundle to the interpreter executable
  (`cli/src/cmd/build.rs:19-36`); the "standalone binary" still interprets.
- **License surprise**: `LICENSE.md` scopes the license *by directory* ("The directory a file sits in decides the
  license that applies to it") and lists every current directory as Apache-2.0, but ships only a link to the
  license text and no `NOTICE`. Copying a file into MIT-licensed LotML is allowed; it keeps its Apache-2.0 header,
  LotML must include the Apache-2.0 text and mark changes, and the directory list must be re-checked at copy time.
