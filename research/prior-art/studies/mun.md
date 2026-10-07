# mun — statically typed, hot-reloadable embeddable language: rust-analyzer frontend, LLVM via inkwell, AOT shared libraries

Repo `research/prior-art/repos/mun` (paths below are relative to it; LotML paths start `compiler/`) ·
license MIT OR Apache-2.0 (`Cargo.toml:15`) · last commit 2026-10-02 · last release 0.5.0,
2023-12-28 (`CHANGELOG.md:16`); the `Unreleased` section is empty (`CHANGELOG.md:8-14`) ·
~57.8k lines of Rust in `crates/` (tests and generated AST included): `mun_hir` 16.2k,
`mun_syntax` 11.0k, `mun_codegen` 8.6k, `mun_memory` 6.9k, `mun_runtime` 6.3k,
`mun_language_server` 3.8k, `mun_abi` 2.2k, `mun_runtime_capi` 1.8k, `mun_compiler` 1.2k,
`mun_target` 1.2k; plus 3.5k lines of C/C++ headers (`c/`, `cpp/`) · Maturity: a pre-1.0
research language with a small user base. The architecture is careful, but releases have been rare
since 2023 and LLVM is still pinned at 14.

## Architecture

The pipeline is one chain of salsa query groups. Each layer is a trait that extends the one below
it, and `mun_compiler` stacks them all into one `CompilerDatabase`:

| Layer | Query group | Key queries |
|---|---|---|
| inputs | `SourceDatabase` (`crates/mun_hir_input/src/db.rs:11-38`) | `file_text`, `file_source_root`, `packages`, `source_root` (inputs); `module_tree(package)`, `line_index(file)` |
| syntax | `AstDatabase` (`crates/mun_hir/src/db.rs:25-34`) | `parse(file)` builds a rowan green tree; `ast_id_map(file)` |
| interning | `InternDatabase` (`crates/mun_hir/src/db.rs:39-49`) | `intern_function`, `intern_struct`, ... give stable ids |
| defs | `DefDatabase` (`crates/mun_hir/src/db.rs:51-90`) | `item_tree(file)`, `fn_data(fn)`, `struct_data`, `package_defs(pkg)` (name resolution), `body(def)`, `expr_scopes(def)` |
| types | `HirDatabase` (`crates/mun_hir/src/db.rs:92-125`) | `target` (input), `infer(def)` per function body, `callable_sig`, `type_for_def`, `lower_struct`, `inherent_impls_in_package` |
| codegen | `CodeGenDatabase` (`crates/mun_codegen/src/db.rs:16-38`) | `optimization_level` (input), `module_partition()`, `target_machine()`, `assembly_ir(group)`, `target_assembly(group)` |

- **Incremental codegen exists, but it is coarse.** `target_assembly(ModuleGroupId)` is a salsa
  query (`crates/mun_codegen/src/db.rs:35-37`). The partition is one group per module
  (`crates/mun_codegen/src/module_partition.rs:75-90`), so one edit reruns codegen for the module
  that changed. Other modules rerun only if a query they read changed. Calls across modules go
  through a dispatch table (see Backend), so a body edit in module A does not invalidate B's IR.
  Salsa caches no LLVM objects, only temp files (`crates/mun_codegen/src/db.rs:8-15`). Each rerun
  creates a fresh `inkwell::Context`, emits IR, writes an object to memory and links it into a
  `NamedTempFile` (`crates/mun_codegen/src/assembly.rs:89-125`).
- **Change detection is by temp-file identity, not content.** `TargetAssembly: PartialEq`
  compares temp paths (`assembly.rs:66-70`). The driver copies an assembly to the output only when
  that path differs from the one it last wrote (`crates/mun_compiler/src/driver.rs:413-455`). A
  query that reruns therefore always counts as changed: there is no backdating at the codegen
  layer.
- **`build` and hot reload share everything.** There is no interpreter or JIT. `mun build` writes
  `.munlib` shared libraries (`assembly.rs:75`). `mun_compiler_daemon` runs the same `Driver` in a
  file-watch loop (`crates/mun_compiler_daemon/src/lib.rs:36-131`). The host loads the libraries
  through `mun_runtime`.
- **How they compare with LotML.** LotML has two backends behind one IR (adr:0025), and its salsa
  layer covers only `parse`, `checked` and `diagnostics`, one query per whole file
  (`compiler/crates/lotml-db/src/lib.rs:29-48`). `lotml build` does not go through the db. It
  writes the runtime and runs clang on every build (`compiler/crates/lotml/src/exec.rs:287-290`).

## Frontend

- **Lexer and parser are rust-analyzer clones** (`crates/mun_syntax/src/lib.rs:1-11`): a
  hand-written lexer (`parsing/lexer.rs`, 136 lines), an event-based recursive-descent parser
  (`parsing/parser.rs`, `parsing/event.rs`), and a lossless rowan CST (`Cargo.toml` dependency
  `rowan`). The typed AST is generated from `src/grammar.ron` into `ast/generated.rs` (2,045 lines).
  A test keeps the generated file up to date (`crates/tools/src/lib.rs:79-84`).
- **Braces, not indentation**, so the indentation work has nothing to compare with LotML's.
- **Errors**: `Parse<T>` always has a tree plus `errors: Arc<[SyntaxError]>` (`lib.rs:44-104`). It
  is tolerant, like LotML's parser.
- **Incrementality granularity**: `ItemTree` (`crates/mun_hir/src/item_tree.rs:52-71`) holds only
  the items of a file and refers to AST nodes by `FileAstId` index, not by offset. A body edit
  leaves the `ItemTree` equal, so salsa backdates it and name resolution
  (`package_defs`) of every other file is not recomputed. Bodies are lowered and inferred per
  function (`body(def)`, `infer(def)`, `db.rs:79-103`). LotML re-checks a whole file on any edit
  (`lotml-db/src/lib.rs:36-39`).
- **Multi-file test fixtures**: `//- /path` sections in one string
  (`crates/mun_hir_input/src/fixture/mod.rs:16-80`).

## Semantics and types

- **Signatures are explicit; locals are inferred**, per body (`crates/mun_hir/src/ty/infer.rs:111-123`).
  Unification uses `ena::InPlaceUnificationTable` (`ty/infer/type_variable.rs:3,81-82`). Integer
  and float literals get their own type variables, which default to `i32` and `f64`
  (`infer.rs:129-151`). LotML's literals default to `int`/`f64` in the same way.
- `InferenceResult` maps every `ExprId`/`PatId` to a `Ty`, and records method resolutions
  (`infer.rs:56-67`). A missing entry reads as `Unknown`, so later stages never panic on a broken
  body (`infer.rs:69-85`).
- **No generics.** Codegen still has
  `unimplemented!("cannot yet deal with type parameters in functions")`
  (`crates/mun_codegen/src/ir/ty.rs:299-302`). HIR types map straight to LLVM types with no
  monomorphization pass. LotML already has `lotml-ir/src/mono.rs`.
- **Memory kind is part of the type**: `struct(gc)` (heap object behind a handle, the default)
  or `struct(value)` (`README.md:101-106`; `ir/ty.rs:174-191`). Value structs passed through the
  public API are boxed into GC handles (`ir/ty.rs:193-210`). This is the closest analogue of
  LotML's question of how records cross the C ABI.
- Dynamic features: none. The language is statically typed with no reflection inside it.
  Reflection exists only for the host, through the ABI tables.

## IR and passes

- **There is no own mid-level IR**: HIR (`Body` + `InferenceResult`) goes straight to LLVM IR
  through inkwell (`crates/mun_codegen/src/ir/body.rs`, 1,611 lines).
- `gen_file_group_ir` (`ir/file_group.rs:36-167`) builds one module per group holding the
  dispatch table, the type table and the `allocatorHandle` global (`:150-158`). It collects the
  intrinsics used (`intrinsics.rs:20-29`: `new` and `new_array`, the only runtime services).
- `gen_file_ir` emits the function bodies. The two LLVM modules are merged with `link_in_module`
  (`code_gen/assembly_builder.rs:45-53`).
- `gen_reflection_ir` emits `get_info`, `set_allocator_handle` and `get_version`
  (`code_gen/symbols/mod.rs:352-413`).
- Optimization: the LLVM legacy `PassManagerBuilder` at the chosen level, nothing of Mun's own
  (`code_gen.rs:19-26`).
- Validators on HIR: uninitialized access (`mun_hir/src/expr/validator/uninitialized_access.rs`),
  privacy leaks (`CHANGELOG.md:36,44,46`).

## Backend and toolchain

- **inkwell, pinned at LLVM 14** (`Cargo.toml:37`, `crates/mun_codegen/Cargo.toml:24`
  `features = ["llvm14-0", ...]`). The IR uses typed pointers (the snapshot shows
  `i32 ()** getelementptr`, `src/snapshots/mun_codegen__test__multi_file.snap:16`) and the
  legacy pass manager (`code_gen.rs:20`). LLVM 17 removed both. Building the compiler needs LLVM
  14 development libraries, and on Windows also MSVC and the Windows SDK (`README.md:143-162`).
  LotML writes opaque-pointer textual IR for clang ≥ 17 (`compiler/crates/lotml-llvm/src/driver.rs:9-10`)
  and links no LLVM. adr:0021 rejected inkwell for exactly this release lock, and Mun's pin is
  that cost made concrete.
- **Object emission**: `TargetMachine::write_to_memory_buffer(FileType::Object)`, with
  `RelocMode::PIC` (`code_gen/object_file.rs:21-23`; `db.rs:57-66`).
- **Linking with no system linker**: `lld_rs` 140.0 (`Cargo.toml:42`) links LLD's C++ libraries
  into the compiler ("Statically link against liblld instead of spawning as process",
  `CHANGELOG.md:293`). There is one `Linker` per flavor (`crates/mun_codegen/src/linker.rs:41-47`):
  - ELF: `--shared -o` (`:83-102`).
  - Mach-O: `-dylib -adhoc_codesign -syslibroot <sdk> -lSystem` (`:166-201`), followed by an
    ad-hoc re-sign (`assembly.rs:114-122`).
  - COFF on Windows: `/DLL /NOENTRY /EXPORT:get_info /EXPORT:get_version
    /EXPORT:set_allocator_handle /IMPLIB /OUT` (`:232-251`), with no CRT and no import libraries.
    This works only because a munlib calls nothing outside itself: every runtime service arrives
    as a pointer, through the dispatch table filled by the host. The one Windows quirk handled is
    the `_fltused` symbol (`code_gen/assembly_builder.rs:55-63`). Nothing in the tree handles
    `memcpy`, `memset` or `__chkstk` libcalls (no grep hits), which LLVM can emit for large copies
    or frames: a latent limit of the no-CRT approach.
- **Calling convention / C ABI.** Exported functions use the C convention. Mun lowers the C ABI by
  hand where it matters: `get_info` returns a struct, so on Windows it takes an `sret` pointer
  parameter built manually (`code_gen/symbols/mod.rs:428-457`). Functions whose signature is not
  marshallable get a public wrapper (`ir/file_group.rs:64-74`).
- **Dispatch table** (`ir/dispatch_table.rs:23-35`). Every call to an `extern` function or to a
  function in another module group is a load from a struct of function pointers, followed by an
  indirect call (`:126-151`; `ir/body.rs:1092-1120`). Calls inside a group are direct
  (`module_group.rs:113-119`). The host fills the pointers at link time.
- **Exported reflection tables** (`crates/mun_abi`, C header made by cbindgen into
  `c/include/mun/abi.h`, `crates/tools/src/abi.rs:6-15`):
  - `AssemblyInfo {symbols: ModuleInfo, dispatch_table, type_lut, dependencies}`
    (`assembly_info.rs:7-18`).
  - `ABI_VERSION`, checked at load (`lib.rs:36`; `crates/mun_runtime/src/assembly.rs:117-123`).
  - `TypeId` = `Concrete(Guid) | Pointer | Array` (`type_id.rs:13-22`). A struct GUID is the MD5 of
    `"struct Name{field: type,...}"` (`ir/ty.rs:396-415`; `mun_abi/src/lib.rs:44-53`), so type
    identity is structural across separately compiled libraries.
- Targets: 7 hard-coded triples (`crates/mun_target/src/spec.rs:136-144`). Cross-compiling works
  because both LLVM and LLD are in-process.

## Runtime

- **Host-side, in Rust** (`crates/mun_runtime`): load, link, invoke, hot reload. There is a C API
  (`crates/mun_runtime_capi/src/runtime.rs:118-344`: `mun_runtime_create`,
  `mun_runtime_find_function_definition`, `mun_runtime_get_type_info_by_name`,
  `mun_runtime_update`) and a C++ wrapper (`cpp/include/mun/*.h`).
- **Loading**: each library is first copied to a temp file, so the original can be rewritten while
  loaded (Windows) and each load is distinct (Linux) (`crates/mun_libloader/src/temp_library.rs:16-61`).
  The required symbols are checked (`mun_libloader/src/lib.rs:40-65`). The GC is handed in as an
  opaque allocator handle (`mun_runtime/src/assembly.rs:125-126`).
- **Linking** (`assembly.rs:165-256`): fill the null dispatch entries from the global table,
  retrying until a fixed point. A signature mismatch fails with `Expected: fn ... / Found: fn ...`
  (`:204-233`). Tables are cloned first, so a failed link rolls back (`:286-287`, `:335-339`).
- **Invocation from Rust**: `Runtime::invoke` checks the argument and return types against the
  reflected signature at run time, then calls `fn_ptr` (`mun_runtime/src/lib.rs:792-856`). An
  unknown name gets a "similar name" suggestion (`:812-819`).
- **Hot reload**, end to end:
  1. The compiler writes libraries under a lockfile in the output directory
     (`mun_compiler/src/driver.rs:35-65,386-406`, a busy-wait with 1 s sleeps).
  2. The runtime's `notify` watcher relinks when the lockfile is removed
     (`mun_runtime/src/lib.rs:478-532`).
  3. It loads the changed libraries and their dependencies, then calls `relink_all`
     (`assembly.rs:312-450`): old types are removed from the type table, new ones added, and
     every live GC object of a changed struct is mapped to its new layout (`:389-395`).
  4. The mapping comes from a Myers diff of fields: insert, delete, move, or edit (cast/rename)
     (`crates/mun_memory/src/diff.rs:7-57`, `mapping.rs:16-62`). Objects are rewritten in place
     behind their handles (`gc/mark_sweep.rs:675-678`). This is why a GC struct is `Foo**`
     (`ir/ty.rs:177-185`): the handle is stable and the payload moves.
- **Memory management: precise mark-and-sweep**, inferred to be chosen for hot reload, since
  remapping needs every live object of a type (`mark_sweep.rs:625+`):
  - The GC keeps every object in a `HashMap` behind one `RwLock`. Each allocation takes the write
    lock, inserts, and allocates a pinned `ObjectInfo` box plus the payload
    (`mark_sweep.rs:471-487`).
  - Roots are only objects the host rooted (`roots > 0`, `:530-546,564-574`). Mun stack frames are
    never scanned, so `gc_collect` is safe only between host calls (`mun_runtime/src/lib.rs:545-550`).
    There are no stack maps and no LLVM GC statepoints: they avoid the hard part of AOT tracing GC
    by restricting when collection may run.
- Strings: none in the language (`value/string.rs` holds only C strings for the ABI tables).
  Arrays are heap objects allocated through the `new_array` intrinsic (`intrinsics.rs:24-28`).
  No exceptions, no CPython interop.

## Testing and conformance

- **insta snapshots of LLVM IR** per module group, optimized and unoptimized, with diagnostics in
  place of IR when the program does not check (`crates/mun_codegen/src/test.rs:1072-1144`). There
  are 132 `.snap` files in `crates/mun_codegen/src/snapshots/`.
- **Inference dumps**: every expression as `range 'text': type`, in inline snapshots
  (`crates/mun_hir/src/ty/tests.rs:10-58`). This is the rust-analyzer pattern, and it catches
  silent inference changes that diagnostic-code tests miss.
- **ABI snapshot**: build a library, load it, and snapshot `get_info()` as RON
  (`crates/mun_codegen/tests/abi.rs:30-31`).
- **Runtime integration tests** that compile, load, invoke and reload (`crates/mun_runtime/tests/`,
  `memory.rs` alone is 2,512 lines; `hot_reloading.rs:27-55`).
- **Doc-tests of the mdbook** through `mun_skeptic` (`crates/mun_skeptic/src/lib.rs:1-3,38`).
- **Generated-file freshness** checks: syntax, ABI header, C API header (`crates/tools/src/lib.rs:20-34,79-99`).
- A Miri script (`CHANGELOG.md:21`) and C++ tests (`cpp/tests/`). No fuzzing, no differential
  testing.

## Reusable for LotML

| Item | Path in repo | What it gives | LotML crate | License verdict | Effort |
|---|---|---|---|---|---|
| Multi-file fixture parser `//- /path` | `crates/mun_hir_input/src/fixture/mod.rs:16-226` | many files in one test string | `lotml-db`/`lotml-ide` tests, once programs span files | adapt with attribution (MIT/Apache) | S |
| Inference-dump test format | `crates/mun_hir/src/ty/tests.rs` (`infer()` helper + inline snapshots) | pins the type of every expression, not just diagnostic codes | `lotml-check/tests` (`Checked` already has every expression's type) | idea / adapt | S |
| IR-per-module snapshot harness | `crates/mun_codegen/src/test.rs:1080-1144` | snapshot of emitted IR, with diagnostics substituted on error | `lotml-ir/tests` (IR text, `lotml-ir/src/text.rs`) and `lotml-llvm/tests` | adapt; `insta` would be a new `docs/stack.md` entry (or see the starlark study for a dependency-free 75-line golden helper) | S |
| Copy-then-load library loader | `crates/mun_libloader/src/temp_library.rs:51-61` | load a DLL without locking it on Windows | only if LotML ever loads its own `--shared` output in-process (tests, harness) | copy with attribution | S |
| Structural type GUID | `crates/mun_codegen/src/ir/ty.rs:396-415` | MD5 of `struct Name{f: T,...}`: a cross-library type identity that changes when the layout does | `lotml-llvm/src/export.rs`, if records ever cross the C ABI | adapt | S |
| ABI version + reflection export | `crates/mun_abi/src/lib.rs:34-42`, `assembly_info.rs:7-18`, `crates/mun_runtime/src/assembly.rs:117-123,204-233` | host can reject a mismatched library and check signatures at load | `lotml-llvm/src/export.rs` | idea (LotML exports are plain C symbols plus a header) | S |
| Target spec table | `crates/mun_target/src/spec.rs:16-154` | triple, data layout, linker flavor, per target | `lotml-llvm` if cross-compilation is ever wanted | idea only (clang's own `--target` covers it) | M |
| Freshness checks for generated files | `crates/tools/src/lib.rs:20-34` | fails the tests when a generated header is stale | nothing: `compiler/crates/lotml-runtime/src/abi.rs:1-3` already reads `lotml.h` itself, so no second record exists | — | — |

## Ideas and optimizations worth adopting

1. **Cache the compiled C runtime instead of recompiling it on every build.**
   - Today `lotml build` writes `lotml.c` and its includes, then makes clang compile them with the
     program (`compiler/crates/lotml/src/exec.rs:287-290`; `lotml-llvm/src/driver.rs:164`). The
     same happens for every program in the LLVM test suite.
   - Mun's split is the model: the runtime is built once, and a generated library holds only user
     code (`linker.rs:232-251`).
   - Compile `lotml.c` once into an object file. Key it on the clang version
     (`driver::Clang.version`), the flags (`-O0`/`-O2`, `-g`, `-DLT_COUNT_CELLS`, sanitizer,
     `-fPIC`/visibility for `--shared`) and a hash of `lotml_runtime::FILES`.
   - Program and runtime are already separate translation units (two inputs to one clang call),
     so caching loses no inlining.
   - Also hoist `driver::find()`, which runs `clang --version`, out of the per-file loop
     (`exec.rs:346`).
   - Expected impact: every native build and the whole LLVM test suite.
   - ADR: adr:0025 says the runtime is "compiled by `clang` with every program". A cache keeps that
     substance (same clang, same flags), but the wording should be noted when this lands.
2. **Finer salsa granularity on the rust-analyzer/Mun pattern.**
   - Add a per-file *signature* query that excludes body text (like `ItemTree`, `item_tree.rs:52-71`,
     with positions held as indices, not offsets), then per-function check, lower and emit queries
     (`mun_hir/src/db.rs:79-103`).
   - Put `lotml build`'s lowering and LLVM emission into `lotml-db` as tracked functions returning
     **text**. Mun stores no LLVM handles in salsa (`mun_codegen/src/db.rs:8-15`), and text is
     what LotML has anyway.
   - Unlike Mun, make results compare by content so salsa backdates them. Mun's assemblies compare
     by temp path, so a rerun always counts as changed (`assembly.rs:66-70`).
   - Payoff: the agent's edit-check loop and LSP latency on large files. It matters more once
     programs span files.
   - ADR: fits adr:0006 and adr:0021 (Salsa).
3. **Snapshot tests that pin intermediate results.**
   - Inference dumps for `lotml-check` (`ty/tests.rs:10-58`).
   - IR text after each pass of `lotml_ir::native` (`own`, `reuse`, `hoist`,
     `compiler/crates/lotml-ir/src/lib.rs:18-27`).
   - The generated `--shared` header (as `tests/abi.rs:30-31` snapshots `get_info`).
   - Today's tests assert diagnostic codes (`compiler/crates/lotml-check/tests/types.rs:6-21`) or
     run programs, so a silent change in inferred types or in where counts are inserted shows up
     only as a later behaviour or performance change.
   - No ADR. A new dev-dependency is a `docs/stack.md` entry.
4. **When the adr:0024 ownership ceiling lifts, generate C shims rather than hand-lowering the C
   ABI in IR.**
   - Mun hand-builds Windows `sret` for one struct-returning export (`symbols/mod.rs:428-457`).
     Every aggregate in a signature would need the same per-platform care.
   - LotML already compiles C with clang on every build, so a generated `<module>_exports.c` that
     passes records by pointer to internal IR functions gets ABI lowering from clang for free.
     `lotml-runtime/src/abi.rs:25-26` already marks structs as something "LLVM IR cannot pass the
     way C does".
   - ADR: touches adr:0013 and adr:0024. No conflict, but it needs its own record.
5. **Records across the C ABI as counted handles.**
   - Mun boxes value structs into heap handles in its public API (`ir/ty.rs:193-210`) and lets
     hosts root and unroot them (`mark_sweep.rs:530-546`).
   - With reference counting, the C analogue is `lt_cell*` plus generated `<module>_<Record>_retain`,
     `_release` and field getters.
   - **Conflict:** adr:0024 explicitly rejects "exporting every function with opaque handles for
     the types that cannot cross". Mun is evidence that the handle design works, but adopting it
     means a new ADR superseding that part of 0024.
6. **ABI version symbol and signature check for `--shared` libraries.**
   - Export `<module>__lotml_abi()` returning a version number, optionally with a table of exported
     names and C signatures, as Mun does with `get_version` and `get_info`
     (`mun_abi/src/lib.rs:36-42`).
   - A host such as Python ctypes, a test harness, or a future LotML loader can then refuse a stale
     library with a readable "Expected/Found" message (`assembly.rs:221-232`).
   - ADR: additive to adr:0024. One more symbol, no syntax, but it changes the exported surface,
     so record it.
7. **Not recommended: hot reload through a dispatch table.**
   - It costs a load and an indirect call per cross-module call (`dispatch_table.rs:126-151`).
   - It needs every live object of a type to be enumerable and relocatable, which is why Mun's
     handles are `Foo**` and its GC keeps an object table. Per-cell reference counting
     (adr:0003, adr:0008) has neither.
   - `lotml run` already reloads cheaply on CPython (adr:0025).
   - **Conflict:** making objects relocatable would conflict with adr:0003/0008.
8. **Not recommended: embedding LLD.**
   - It means linking LLVM/LLD libraries into `lotml`. **Conflict:** adr:0021 and adr:0025 decided
     "no linked LLVM".
   - It would not remove LotML's Windows toolchain dependency anyway. Mun's no-CRT DLLs
     (`/NOENTRY`, `linker.rs:241-242`) work only because generated code calls nothing outside
     itself, while a LotML executable needs a C runtime (`printf`, `malloc`, ...), so the CRT and
     SDK libraries would still be needed.

## Pitfalls seen

- **Release lock on linked LLVM.**
  - LLVM bumps were each a PR (11→13→14, `CHANGELOG.md:26,73`), and the last was in 2023.
  - The emitter uses typed pointers (`build_struct_gep(ptr, idx)` with no pointee type,
    `dispatch_table.rs:137-148`) and the legacy pass manager (`code_gen.rs:20`). Both are gone in
    LLVM 17.
  - Contributors need LLVM 14 development files (`README.md:153-162`).
- **Codegen cache without backdating** (`assembly.rs:66-70`; `driver.rs:435-445`). Any rerun of
  `target_assembly` rewrites the `.munlib`, which triggers a reload.
- **Hot reload may leave stale pointers in dependent libraries.** This is a reading of the code,
  not a reproduced bug.
  - `relink_all` links only the new assemblies' null dispatch entries (`assembly.rs:405-422`) and
    then drops the old assembly, which unloads its library (`:436-444`).
  - An unchanged dependent that called into the reloaded library keeps its old function pointers.
  - The multi-file reload test passes `force = true`, which rewrites every library
    (`crates/mun_test/src/driver.rs:108`; `crates/mun_runtime/tests/hot_reloading.rs:27-55`). The
    daemon uses `false` (`crates/mun_compiler_daemon/src/lib.rs:73`), so that path is not the one
    tested.
  - There is also a `// TODO: don't overwrite existing` in the event handler
    (`mun_runtime/src/lib.rs:506`).
- **Claim with no switch behind it.** The README says the hot-reload overhead "can be disabled for
  production builds" (`README.md:54-56`). No such option exists in `crates/` (grep for
  `hot_reload`/`reload` flags finds none).
- **Expensive allocation.** Every `new` is an indirect call through the dispatch table into Rust,
  takes a global `RwLock` write, inserts into a `HashMap`, and makes two heap allocations
  (`mark_sweep.rs:471-487`). Every field access through a GC struct is a double indirection
  (`ir/ty.rs:177-185`). These are the allocation-heavy costs adr:0008 already worries about, made
  worse.
- **GC only between host calls** (`mark_sweep.rs:564-574`). A long-running Mun call cannot
  collect. Tracing GC in an AOT language without stack maps is only safe this way, which supports
  adr:0003's rejection of a tracing collector.
- **Generics never arrived** (`ir/ty.rs:299-302`). HIR maps directly to LLVM types with no
  monomorphization stage to add them in. LotML's `mono.rs` between checker and backends avoids
  this.
- **Windows needs more than the embedded linker.** Building the compiler still requires MSVC and
  the Windows SDK (`README.md:156-157`). An embedded linker removes the *user's* linker
  dependency, not the developer's.
