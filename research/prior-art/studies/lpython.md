# lpython — typed-Python AOT compiler in C++ over LFortran's ASR, with LLVM/C/C++/WASM/x86 backends and a JIT REPL

`lcompilers/lpython` · BSD-3-Clause (`LICENSE`, Triad National Security) · last commit 2025-12-11
(`git -C research/prior-art/repos/lpython log -1 --format=%cs`) · README calls it "alpha stage"
(`README.md:3-4`).

**Source note.** The clone holds only the Python front end. Everything below the AST lives in
`libasr/`, an **uninitialized git submodule** (`.gitmodules`; `git ls-tree HEAD libasr` → commit
`f2a04fc2ae4dd9ccb940c0fee3f694791fec7005` of `lfortran/lfortran`). It holds the ASR definition,
pass manager, every backend and the C runtime. I read those files read-only from GitHub at that
pinned commit. I cloned nothing, ran nothing and wrote nothing outside this file. Citations:

- `path:line`: relative to `research/prior-art/repos/lpython/`, checked in the clone.
- `[L] path:~line`: `lfortran@f2a04fc:src/libasr/path`. The `~` means the line came through a
  fetch tool that renumbers, so it can be off by up to ±5. The symbol is named so it can be found.

Size: front end `src/lpython` 16.8k lines of C++ (`semantics/python_ast_to_asr.cpp` alone has
9,259), driver `src/bin/lpython.cpp` 2,338, runtime written in LPython `src/runtime/*.py` 5.3k,
integration tests 455 `.py` files / 18.7k lines. The libasr files below are sized in bytes, from
the GitHub contents API: `codegen/asr_to_llvm.cpp` 824 KB, `codegen/llvm_utils.cpp` 503 KB,
`asr_utils.h` 317 KB, `pass/intrinsic_functions.h` 356 KB, `runtime/lfortran_intrinsics.c` 200 KB,
`asdl_cpp.py` 149 KB, `codegen/asr_to_c_cpp.h` 131 KB, `codegen/c_utils.h` 104 KB, `ASR.asdl`
18 KB (~280 lines).

Maturity: a broad but brittle research compiler. 188 of 400 integration tests are commented out,
39 of them marked "post sync" (`integration_tests/CMakeLists.txt`). The disabled ones include
every class test, every generics test and every CPython-interop test (Pitfalls §1).

## Architecture

Pipeline (driver `src/bin/lpython.cpp`):

1. **Parse**: `parse_python_file` (`src/lpython/parser/parser.cpp:115-134`). It runs the re2c
   tokenizer (`parser/tokenizer.re`) and the bison grammar (`parser/parser.yy`) and produces an
   AST generated from `grammar/Python.asdl` (`build0.sh:11`).
2. **AST → ASR**: `python_ast_to_asr` (`semantics/python_ast_to_asr.cpp:9138-9257`) runs two
   visitors. `SymbolTableVisitor` (`:4315`) declares, and `BodyVisitor` (`:5124`) fills bodies.
   The main module is then wrapped into a `Program` that calls `__main__global_stmts`
   (`:9210-9254`). Imports are compiled into the same translation unit (`load_module`,
   `:174-289`).
3. **ASR → ASR passes**: `PassManager` (`[L] pass/pass_manager.h:~76-430`). Which passes run
   depends on the backend (`use_default_passes(true)` skips `_c_skip_passes` for C,
   `src/bin/lpython.cpp:321`).
4. **Backends**: `asr_to_llvm` (the passes run *inside* it: `[L] codegen/asr_to_llvm.cpp`, entry
   function at the end of the file calls `pass_manager.apply_passes`), `asr_to_c`
   (`src/bin/lpython.cpp:326-329`), `asr_to_cpp`, `asr_to_wasm`, `wasm_to_x86/x64`
   (`:1396-1421`), `asr_to_x86`, `asr_to_python` (`:433`, reads the ASR *before* passes).
5. **Link**: `link_executable` (`:1434-1625`). It runs `system()` on `link /NOLOGO`
   (MSVC), `cc` (overridable by `LFORTRAN_CC`) or `gcc`, against `lpython_runtime[_static]`.
   `lpython file.py` then runs the result (`:2296-2313`).

How the modes share the pipeline: **AOT** (`lpython f.py`, `-c`) goes parse → ASR → passes →
LLVM module → `save_object_file` (`:1166-1171`). **`--jit`** is the identical pipeline up to the
LLVM module, then `LLVMEvaluator::add_module` and
`execfn("__module___main_____main__global_stmts")` (`:1133-1165`). The **REPL / Jupyter**
(`PythonCompiler::evaluate`, `src/lpython/python_evaluator.cpp:67-295`) runs the same stages once
per cell. **"CPython mode"** involves no compiler at all: the same `.py` file runs on CPython with
`src/runtime/lpython/lpython.py` emulating the annotations and decorators (Runtime section).

The last mode is the deep contrast with LotML. LPython's Python "target" is the identity, so
nothing makes it implement LPython's semantics. LotML's Python target is a real backend from the
IR (adr:0020, adr:0025), so a semantic rule such as the overflow trap reaches both targets.

## Frontend

- **Tokenizer**: re2c (`tokenizer.re`, 857 lines). Indentation uses an explicit stack
  (`indent_length`, `last_indent_length`; INDENT at `:326-347`, DEDENT at `:349-366`, extra
  DEDENTs at end of input at `:182-193`). Mixing tabs and spaces is an error (`:339-341`,
  `:362-364`). A parenthesis stack suppresses NEWLINE/INDENT inside brackets (`:146-163`, `:314`).
  `# type:` comments are tokens (`:298`, `:573-579`).
- **Parser**: bison LALR(1), pure, `%expect 0` (`parser.yy:1-6`), 1,263 lines.
- **No error recovery.** The first tokenizer or parser error throws (`parser.cpp:20-31`,
  `handle_yyerror` `:60-87`). LotML's tolerant parser is ahead here.
- **AST**: CPython's own ASDL (`grammar/Python.asdl`, 157 lines). It is turned into C++ structs
  by the same generator as ASR. A `--new-parser` flag remains, but is forced on
  (`parser.cpp:122-124`).
- **Memory**: one arena per compilation (`Allocator al(4*1024)`, `src/bin/lpython.cpp:760`).
- **Incrementality: none.** Module ASR can be serialized to `.pyc` (`save_pyc_files`,
  `python_ast_to_asr.cpp:42-58`) and loaded back (`:213-225`). Saving imported modules is
  disabled: "TODO: Uncomment once a check is added for ensuring that module.py file hasn't changed
  between builds" (`:146-149`). Precompiling intrinsic modules is off by default
  (`CMakeLists.txt:85-86`). LotML's salsa queries solve what LPython left open.

## Semantics and types

- **Types come only from annotations.** The built-in names are `i8..i64`, `u8..u64`, `f32`,
  `f64`, `c32`, `c64`, `str`, `bool`, `CPtr`, `Pointer[...]` and `S` (SymEngine)
  (`python_ast_to_asr.cpp:854-907`). `int` and `float` are *rejected* with the hint "Use i8, i16,
  i32 or i64 for now" (`:909-927`). An integer literal is `i32` (`:3529-3533`). Every parameter
  needs an annotation (`:4607-4609`). Parameters default to intent `In`, and arrays to `InOut`
  (`:4645-4650`).
- **Decorators drive the ABI** (`:4524-4584`). `@ccall` (optionally `header=`) makes a BindC
  interface; `@ccallable` / `@ccallback` a BindC function with a body; `@pythoncall(module=)` a
  `BindPython` interface and `@pythoncallable` one with a body; `@jscall` makes `BindJS`. Also
  `@overload`, `@vectorize`, `@restriction`, `@inline` and `@static`. `@lpython` is refused ("must
  be run from CPython", `:4551-4554`), and any other decorator is an error (`:4555-4557`).
- **Overloading**: `@overload` renames each body `__lpython_overloaded_<n>__<name>`
  (`:4587-4598`). One `GenericProcedure` is then built per name (`:4789-4797`), and the call is
  resolved by signature.
- **Methods**: a table keyed `"type@method"` maps to ASR-node builders, such as `"list@append"`
  → `eval_list_append` (`semantics/python_attribute_eval.h:21-44`). `modify_attr_set` lists the
  methods that mutate (`:46-48`).
- **Constant folding during semantics**: `comptime_eval_map` (`python_comptime_eval.h:49-100`).
  It covers `pow`, `round`, `bin`, `divmod`, `_mod` and others. A string method that is not yet
  implemented is mapped to `not_implemented`.
- **Generics**: `TypeVar` with `@restriction` (`:1426-1519`). The call-site instantiation is
  **commented out** (`:1206-1239`).
- **Refused Python features**: `raise` becomes `ErrorStop`, i.e. the process exits
  (`:7277-7286`). There is no `try`, and no visitor for comprehensions or generators. Sets must be
  homogeneous and hashable (`:7288-7313`). Mutability is enforced only for `Const` and intents.

## IR and passes

### What ASR is

ASR is defined in `[L] ASR.asdl` (~280 lines). `[L] asdl_cpp.py` (149 KB of Python) turns it
into `asr.h` during the build (`build0.sh:12-13`). It holds `TranslationUnit(symbol_table
symtab, node* items)` (`~:9`), ~15 `symbol` kinds (`~:12-29`: `Program`, `Module`, `Function`,
`GenericProcedure`, `ExternalSymbol`, `Struct`, `Variable`, ...), ~53 `stmt` kinds (`~:32-84`),
~114 `expr` kinds (`~:87-200`) and 20 `ttype` kinds (`~:202-222`). Each scope owns a
`symbol_table`, and a `Function` carries a `dependencies` list.

Against the **AST**: names are resolved into symbol pointers (`Var(symbol v)`, `~:152`), and
imports become `ExternalSymbol` (`~:17`). Every `expr` carries `ttype type` **and** `expr? value`,
its compile-time constant (`IntegerBinOp(..., ttype type, expr? value)`, `~:104`). Implicit
conversions are explicit `Cast(expr arg, cast_kind kind, ...)` nodes, with 33 cast kinds (`~:177`,
`~:224`), and overloads are explicit (`OverloadedBinOp`). Containers are built-in nodes:
`ListAppend`, `DictInsert`, `ListConcat`, `DictItem`, `StringFormat(kind = FormatPythonFString...)`
(`~:70-82`, `~:123-146`).

Against a **lower IR**: expressions stay trees with no named temporaries, and control flow stays
structured (`DoLoop`, `WhileLoop`, `If`, `Select`, `ForEach`). There is no ownership or count
operation, only `ImplicitDeallocate` / `ExplicitDeallocate`. Lowering is ASR → *a subset of* ASR:
each pass removes some node kinds, and each backend reads the subset left after its passes.

The schema is Fortran-first. It has `FileInquire` with 33 fields, `OMPRegion`, `SelectRank`,
`Where`, `ArrayPhysicalCast` and nine array physical layouts (`~:245`), and booleans dump as
`.false.` (`tests/reference/pass_print_list_tuple-print_02-09600eb.stdout`).

### Compared with LotML's IR

LotML's single IR (adr:0020, `compiler/crates/lotml-ir/src/ir.rs`) is one level *lower* than ASR.
Every intermediate value is a named `Local` (`ir.rs:5-8`), and every statement carries its span.
Runtime operations are one `Builtin` enum named for the operation (~120 variants,
`ir.rs:11-134`), not one node kind per operation. Counting is explicit
(`StmtKind::Inc/Dec/DropReuse`, `ir.rs:369-380`). There are no symbol tables: the checker has
resolved everything, so the IR holds strings and types.

What ASR gets right and LotML already shares: the Python backend reads the program *before*
lowering. LPython's `asr_to_python` reads ASR before the passes; LotML's Python backend reads the
IR before `mono` and `native`. LotML does two things better. **Which form a backend reads is
explicit**: `lotml_ir::native()` runs for LLVM only (`lotml-ir/src/lib.rs:17-27`,
`lotml-llvm/src/lib.rs:80-81`), whereas LPython lets each backend skip passes through string
lists (`_c_skip_passes`), so its contract is implicit. **Verification is always on**: LotML
verifies after every pass (`verify::assert_valid`), while LPython's `asr_verify` after a pass runs
only under `WITH_LFORTRAN_ASSERT` (`[L] pass/pass_manager.h:~189-195`).

### Is a code-generated IR definition (ASDL) worth adopting for `lotml-ir`? No.

From one `.asdl`, `asdl_cpp.py` generates node structs and arena `make_*` constructors;
`BaseVisitor`, `BaseWalkVisitor` and the scope-aware `ASRPassBaseWalkVisitor`; expression and
statement replacers; `ExprStmtDuplicator` (a deep copy); pickle, JSON, tree printing, binary
serialization and deserialization; and the `expr_type()` / `expr_value()` accessors. That matters
in C++, which has no derives. In Rust, `#[derive(Clone, Debug, PartialEq, Hash)]` already gives
the duplicator, the dump and equality; serde is in `docs/stack.md` if the IR ever needs
serializing; and `text.rs` is the printer the IR's tests compare (R1.5).

The generator also costs LPython a Python build step ahead of CMake (`build0.sh:8-17`). For
LotML, that would be a second toolchain in a pure-cargo build.

The one thing LotML lacks is **generated walkers**. `Expr::operands` exists (`ir.rs:736-822`),
but there is no `operands_mut` and no block/statement walker. So `own.rs` (28 `Expr::` and 51
`StmtKind::` matches), `hoist.rs`, `reuse.rs`, `verify.rs` and `mono.rs` each hand-roll their
traversal. Adopt the idea, not the tool: a `visit.rs` with `walk_block` / `walk_block_mut`, and
`operands_mut`. A `macro_rules!` can emit the `&` and `&mut` variants from one body, as rustc's
MIR visitor does. Effort S, no new dependency.

### ASR → ASR passes

The registry is `_passes_db` (`[L] pass/pass_manager.h:~83-123`). `--pass=` / `--skip-pass=`
select passes by name (`~:215-237`, `~:298-304`).

- **Default passes**, in order (`~:242-275`): global_stmts, init_expr,
  function_call_in_declaration, openmp, implied_do_loops, array_struct_temporary,
  transform_optional_argument_functions, nested_vars, forall, class_constructor, pass_list_expr,
  where, array_op, symbolic, intrinsic_function, intrinsic_subroutine, subroutine_from_function,
  **array_op again**, pass_array_by_data, array_passed_in_function_call, print_struct_type,
  print_arr, print_list_tuple, **print_struct_type again**, array_dim_intrinsics_update,
  do_loops, while_else, select_case, unused_functions, unique_symbols, insert_deallocate.
- **`--fast` adds** (`~:276-286`): replace_with_compile_time_values, loop_vectorise,
  dead_code_removal, unused_functions, sign_from_value, div_to_mul, fma, inline_function_calls,
  promote_allocatable_to_nonallocatable. LLVM O3 then runs as well
  (`python_evaluator.cpp:474-476`; `[L] codegen/evaluator.cpp` `opt()`: `PassBuilder` O3 on LLVM
  ≥ 17, legacy `PassManagerBuilder` otherwise).
- **Skipped for C**, "already handled appropriately in C backend" (`~:290-297`):
  replace_with_compile_time_values, pass_list_expr, print_list_tuple, do_loops, select_case,
  inline_function_calls.
- **Instrumentation**: `--time-report` times each pass (`~:196-208`). `--dump-all-passes` writes
  `pass_NN_<name>.clj`, plus JSON, tree, an HTML visualization and Fortran, after every pass
  (`~:335-412`). `--cumulative` runs every pass up to the requested one (`~:136-170`).

The passes that bear on LotML:

| Pass (`[L] pass/`) | What it does | For LotML |
|---|---|---|
| `inline_function_calls.cpp` | Inlines any call that passes `check_inline_possibility` (~105-199). It refuses strings, structs, pointers, allocatables, function-typed params, non-global dependencies, and returns not at the end. Return becomes an assignment to the return var (~381-384), and locals are renamed with `get_unique_name` (~211). | In practice only scalar code is inlined. clang `-O2` already inlines for the LLVM target. Idea only. |
| `unused_functions.cpp` | Reachability from the `Program` (only if one exists, or `always_run`, ~384), iterated up to 4 rounds (~387). BindC/BindPython functions are always kept (~59). Unused members are pruned from `GenericProcedure`. | **Useful**: fewer functions emitted means less `clang -O2` time for `lotml build`. |
| `dead_code_removal.cpp` | Only `If` / `Select` with a constant test (~28-105). | Trivial. LLVM does it. |
| `replace_with_compile_time_values.cpp` | Substitutes each expr's `m_value` constant (~59-66). It skips array/string items and assignment targets (~49-54, ~108). | A pattern to adopt only with the divergence guard of Pitfalls §6. |
| `print_list_tuple.cpp` | Lowers `print([..])` into a loop of scalar prints with `[`, `", "`, `]` and quoted strings (~10-57, ~123-271). | Do not adopt. LotML prints through one runtime `repr` per type (`lt_type.repr`, `lotml.h:511-518`). |
| `pass_list_expr.cpp` | Rewrites `ListSection` / `ListConcat` into generated `_lcompilers_list_section` / `_list_concat` functions, cached per element type (~447-599). | LotML's runtime does this generically. Do not adopt. |
| `nested_vars.cpp` | Closures: captured variables become globals in a synthetic module `__lcompilers_created__nested_context__<f>`, copied in before and out after each call (~9-68, ~526-733). | **Anti-pattern**: not reentrant. LotML's `lt_closure` carries captures (`lotml.h:638-641`). |
| `insert_deallocate.cpp` | `ImplicitDeallocate` for allocatable locals before `Return`/`Exit` and at the end of the body. Loop temporaries (`__libasr_created`) are handled at loop end. | LotML's last-use `Dec` (`own.rs:64`) is finer. Nothing to take. |
| `global_stmts.cpp` | Wraps module-level statements into one function. The last expression becomes its return var, which is how the REPL shows `_` (~79-88). | Only if LotML ever wants a native REPL (it does not, per adr:0025). |
| `while_else.cpp` | `while ... else` becomes a flag variable plus an `if` after the loop (~71-73). | Trivial. |
| `loop_vectorise.cpp` | `for i in range(N): a[i]=b[i]` with constant `N` becomes `vector_copy` on 512-bit chunks (comment ~14-26). | A toy. LLVM's vectorizer covers it. |
| `intrinsic_function.cpp` + `intrinsic_function_registry.h` | Each intrinsic id registers `{instantiate, verify}` and `{create, eval}` callbacks (`{"sin", {&Sin::create_Sin, &Sin::eval_Sin}}`, ~260, ~600). `instantiate` writes the intrinsic's body *in ASR*, so backends without a runtime still work. | LotML maps `Builtin` to C runtime calls. Idea only. |
| `implied_do_loops`, `array_op`, `where`, `forall`, `openmp`, `select_case`, `do_loops`, `unique_symbols`, `class_constructor`, ... | Fortran array semantics, mangling and lowering. | Not relevant. |

## Backend and toolchain

### LLVM (`[L] codegen/asr_to_llvm.cpp`)

- **Structure**: `class ASRToLLVMVisitor : public ASR::BaseVisitor<ASRToLLVMVisitor>` (CRTP),
  one 824 KB file plus `llvm_utils.cpp` (503 KB) and `llvm_array_utils.cpp` (76 KB). The LLVM C++
  API is linked into the compiler (`CMakeLists.txt:140-190`: core, mcjit, orcjit, native...).
- **Version churn**: the code is full of LLVM-version branches. `opt()` splits at LLVM 17, and
  `create_global_string_ptr` carries a workaround for LLVM ≤ 7 (`[L] codegen/llvm_utils.h`).
- **Strings**: a descriptor `{i8*, i64 length}` (`[L] codegen/llvm_utils.cpp` constructor,
  `string_descriptor_members`). `str` is `Allocatable String(DeferredLength, DescriptorString)`
  (`python_ast_to_asr.cpp:890-893`). Assignment copies through `lfortran_str_copy`.
- **Lists**: `{i32 current_end_point, i32 capacity, T* data}` (`LLVMList::get_list_type`),
  growing to `2*capacity+1`.
- **Dicts**: `LLVMDictOptimizedLinearProbing` is `{i32 occupancy, key_list, value_list, i8*
  key_mask}`; `LLVMDictSeparateChaining` is `{occupancy, filled_buckets, capacity, kv_pairs*,
  mask*, i1 rehash_flag}`. **String keys use chaining, all other keys linear probing**
  (`LLVMUtils::set_dict_api`); sets always probe linearly. **Integer hash = `key urem capacity`**;
  the string hash is polynomial (`p = 31`, `m = 100000009`). Rehash at load ≥ 0.6
  (`5*occupancy >= 3*capacity`). Iteration order is not insertion order.
- **Tuples and structs**: tuples are LLVM structs (`LLVMTuple`). Structs are LLVM structs, with a
  vptr for classes (`LLVMStruct::store_class_vptr`).
- **Memory**: there is no counting. **Every assignment deep-copies** (`list_api->list_deepcopy`,
  `tuple_api->tuple_deepcopy`, `dict_api->dict_deepcopy`, `llvm_utils->deepcopy` for structs, in
  `visit_Assignment`). Freeing happens at scope end through `insert_deallocate` /
  `visit_ImplicitDeallocate` and `LLVMFinalize::finalize_symtab`. The whole module is checked with
  `llvm::verifyModule` before it is returned.
- **C ABI**: BindC calls pass value arguments by value. Complex numbers get per-platform
  handling: Windows passes `i64`, and macOS ARM uses arrays (asr_to_llvm, chunk 600-700 k).

### Object files, JIT and linking

- **Objects**: `TargetMachine::addPassesToEmitFile`. The CPU is hard-coded to `"generic"` with no
  features (`[L] codegen/evaluator.cpp` ~122-128, 242-256).
- **JIT**: `llvm::orc::KaleidoscopeJIT`, the LLVM tutorial JIT (`[L] codegen/KaleidoscopeJIT.h`;
  `evaluator.cpp` ~56, ~141, `add_module` ~184-196). Runtime symbols resolve because
  `lpython_runtime_static` is linked into the compiler itself (`src/lpython/CMakeLists.txt:29`).
- **Linking**: Windows MSVC runs `link /NOLOGO /OUT:... lpython_runtime_static.lib`
  (`lpython.cpp:1502-1512`); elsewhere `cc ... -llpython_runtime -lm` with rpath (`:1514-1535`);
  the C backend always uses `gcc` (`:1583`). `--enable-cpython` links `-lpython3.10` from
  `$CONDA_PREFIX`, version hard-coded (`:1540-1547`, `:1599-1606`). Cross-compilation is only
  `--target` passed to LLVM, with no cross linker.

### C (`[L] codegen/asr_to_c.cpp`)

- **Structure**: `class ASRToCVisitor : public BaseCCPPVisitor<ASRToCVisitor>`, shared with the
  C++ backend through `asr_to_c_cpp.h`.
- **Containers are emitted as per-type C** by `CCPPDSUtils` (`[L] codegen/c_utils.h`). Lists are
  `struct list_<T> { int32_t capacity; int32_t current_end_point; T* data; }`, **a different
  field order from LLVM's list**, growing to `2 * capacity + 1`. Dicts are parallel arrays
  `{K *key; V *value; int capacity; bool *present;}`; integer keys probe from `k % capacity`, and
  **other keys scan every slot**: `for (int i=0; i<x->capacity; i++) if (x->present[i] &&
  key_cmp) return x->value[i];`. Tuples are `struct { int32_t length; elements... }`; strings
  are `char*`.
- **`main`** calls `_lpython_set_argv(argc, argv)`. With `enable_cpython` it includes `Python.h`
  and calls `Py_Initialize` / `Py_FinalizeEx`; with `link_numpy`, `_import_array()`.
- **Python bindings**: `BindPyUtils` generates `conv_py_arr_to_c_*`. When the value is not a
  numpy array, it only prints "Return value is not an array" and carries on.

## Runtime

### C runtime

`[L] runtime/lfortran_intrinsics.c` (200 KB) and `.h` (18 KB) are compiled as
`lpython_runtime` and `lpython_runtime_static` (`src/runtime/legacy/CMakeLists.txt:1-22`), exported
with `LFORTRAN_API`. Printing is `_lfortran_printf` (`fwrite` + `fflush`) and
`_lcompilers_string_format_fortran`. `_lfortran_float_to_str4` is `sprintf(res, "%f", num)` into
`malloc(40)`, **not Python's `repr`**. `_lfortran_strcat` allocates a buffer the caller owns;
`_lfortran_str_slice` follows Python's rules for negative indices and steps. Also: memory
(`_lfortran_malloc/realloc/free`), `_lpython_set_argv/get_argv` and
`_lpython_call_initial_functions` (argv, then `_lfortran_init_random_clock`), random via `rand()`,
libm wrappers (`_lfortran_dsin`, `_lfortran_dgamma`, ...). Stack traces: `print_stacktrace_addresses`
bisects a `lines.dat` table made at build time by `llvm-dwarfdump` plus two Python scripts
(`lpython.cpp:2264-2287`).

### Builtins written in LPython itself

`src/runtime/lpython_builtin.py` (1,147 lines, 221 definitions) holds the builtins, for example
`abs` as 9 `@overload`s (`:9-77`) and `round` (`:279-331`). It is compiled together with each
program as module `lpython_builtin` (`python_comptime_eval.h:51`). `math.py`, `cmath.py`,
`statistics.py`, `random.py` and `lpython_intrinsic_numpy.py` follow the same approach.

### CPython emulation (`src/runtime/lpython/lpython.py`)

This module lets the unchanged source run on CPython:

- `i32` and the other numeric names are `Type` objects whose call is just `int()` / `float()`
  (`:23-54`, `:81-99`). There is **no width and no wrap-around**.
- `@overload` dispatches at run time on the annotations (`:153-196`).
- `@ccall` becomes `CTypes` (`:337-402`). It loads a shared library named by
  `LPYTHON_PY_MOD_NAME` / `_PATH`, or the runtime's `.so`/`.dylib`/`.dll`. It maps annotations to
  ctypes (`convert_type_to_ctype`, `:259-306`), passes numpy arrays as `data_as(POINTER(...))`
  (`:395-396`), and encodes and decodes UTF-8 strings (`:393-401`).
- A dataclass becomes a `ctypes.Structure`, including `_pack_` and fixed-size array fields
  (`convert_to_ctypes_Structure`, `:425-477`).
- `@pythoncall(module=)` simply `importlib.import_module`s the module (`:488-494`).
- `@lpython` (`:655-759`) is the reverse direction, CPython calling compiled code. It writes the
  function's source with `@pythoncallable` into `./lpython_decorator_<f>/`, runs `lpython
  --show-c --disable-main`, then `gcc -shared` against `libpython`, numpy and the runtime, and
  imports the extension, cached by function object in-process only. Linux and macOS only
  (`:706-711`); it imports `distutils` (`:716`), removed in Python 3.12.

### Native CPython interop and errors

**Native CPython calls**: `@pythoncall` is accepted by the semantics (`BindPython`,
`python_ast_to_asr.cpp:4534-4536`, `4567-4570`), but at this libasr commit no pass lowers a
BindPython call: `pass_manager.h` lists no `python_bind`, and `asr_to_c.cpp` has no
`PyObject_Call`. Every `bindpy_0[1-6]` test is commented out (`CMakeLists.txt:666-671`), so the
direction is dead in this snapshot. **Errors**: `raise` exits the process (`ErrorStop`); runtime
errors print a Python-like message and exit 1 (`tests/reference/runtime-test_list_01-3ee9b3e.stderr`:
"ValueError: The list does not contain the element: 4").

## Testing and conformance

### Integration tests: the backend parity mechanism

The tests live in `integration_tests/CMakeLists.txt`. Each program is listed once:

```
RUN(NAME x LABELS cpython llvm llvm_jit c wasm ... [FAIL] [NOFAST] [EXTRAFILES f.c] [EXTRA_ARGS ...])
```

(macro at `:323-360`). `run_tests.py -b <kinds>` configures one CMake tree per kind with
`-DKIND=<kind>`, builds it and runs `ctest` (`integration_tests/run_tests.py:23-28`). There are
13 kinds (`:9`), each compiling and running the program its own way (`RUN_UTIL`, `:79-321`):
`llvm` is `lpython -c` then a link with the runtime, `llvm_jit` is `lpython --jit`, `c` is
`--show-c` then the C compiler, `cpython` is `python file.py` with `PYTHONPATH` set to the
emulation module (`:226-233`), `wasm` runs under node, and `*_py` / `*_sym` link Python or
SymEngine.

**Pass = exit status 0, or non-zero under `FAIL` (`WILL_FAIL`).** Programs check themselves with
`assert`; **standard output is never compared with CPython**, so `print_float.py`, which only
prints, cannot catch float-formatting drift. With `FAST=yes`, every test not marked `NOFAST` runs
again with `--fast`, `cpython` labels removed (`:354-359`).

Counts in this snapshot: 212 active `RUN`, 188 commented. Labels: cpython 206, llvm 205,
llvm_jit 202, c 183, wasm/wasm_x64/wasm_x86 36/22/12, x86 1. 24 active tests run on llvm but
not c; 55 `# renable c` and 39 `# post sync` comments. The gaps are recorded only as **comments,
which no tool counts**.

### Golden files

`tests/tests.toml` lists small programs under `tests/` (226) and `tests/errors/` (183 files),
each with flags `tokens`, `ast`, `asr`, `llvm`, `c`, `cpp`, `wat`, `pass=` and `run`
(`run_tests.py:12-140`). Outputs are compared with
`tests/reference/<kind>-<name>-<hash>.{stdout,stderr}`. A JSON sidecar stores `infile_hash`,
`stdout_hash` and `returncode` (`runtime-test_list_01-3ee9b3e.json`). There are 455 ASR
references, so every schema change rewrites hundreds of S-expression dumps.

CI (`.github/workflows/CI.yml`) builds on Ubuntu, macOS and Windows. Windows uses MSVC and Ninja
and has runtime stack traces off (`:93-128`). The backend matrices run on Linux only (`:325-607`).

### Compared with LotML

LotML already does the important parts better: the LLVM target is diffed against the Python
target on stdout, stderr and exit code (`lotml-llvm/tests/common/mod.rs:10-19`); every program
runs at both `Level::Debug` and `Level::Release` (`:147-148`), the same doubling as LPython's
`--fast`; and a leak check comes with it (`frees_everything`, `tests/native.rs:22`).

## Reusable for LotML

| Item | Path | What it gives | LotML crate | License verdict | Effort |
|---|---|---|---|---|---|
| dataclass → `ctypes.Structure` (packed, fixed arrays) | `src/runtime/lpython/lpython.py:425-477` (+ numpy pointer passing `:395-396`) | C structs and arrays over ctypes on the Python target | `lotml-py/runtime/lotml_rt.py` (C interface calls, `:557-591`) | BSD: adapt with attribution. **Only once adr:0013's "no structs/arrays" ceiling is lifted by a new ADR.** | S |
| Pass-manager instrumentation: named pass list, `--pass` / `--skip-pass`, per-pass timing, dump after every pass | `[L] pass/pass_manager.h:~83-123, ~132-213, ~335-412` | Debuggable pass pipeline for agents and humans | `lotml-ir/src/lib.rs` `native()` plus `text.rs` | BSD, but C++: idea only, the Rust is about 40 lines | S |
| Reachability pruning (fixed-point from entry, keep exported ABI functions) | `[L] pass/unused_functions.cpp` (~59, ~340, ~384-387) | Fewer functions handed to `clang -O2` | new `lotml-ir` pass before `lotml-llvm` emit; keep adr:0024 exports as roots | Idea only | S-M |
| Per-test backend labels plus a `--fast` doubling | `integration_tests/CMakeLists.txt:323-360` | Declared target coverage per program | LotML parity suite | Idea (LotML already doubles the levels) | S |
| C runtime (strings, lists, dicts, hashing, float formatting) | `[L] runtime/lfortran_intrinsics.c`, `[L] codegen/llvm_utils.cpp`, `c_utils.h` | Nothing better than `lotml-runtime`. LotML already has CPython-exact SipHash-1-3 (`lotml_dict.c:20-70`), an insertion-ordered compact dict (`lotml.h:667-678`) and shortest-round-trip float `repr` (`lotml.c:549-590`). | — | Nothing to take | — |
| JIT and evaluator | `[L] codegen/evaluator.cpp`, `KaleidoscopeJIT.h` | In-process JIT | — | **Conflicts with adr:0025** (no JIT, no linked LLVM, adr:0021) | — |
| ASDL generator | `[L] asdl_cpp.py` | Visitors and serializers | — | Do not adopt. Take only the walker idea (IR and passes section) | — |

## Ideas and optimizations worth adopting

Ranked by expected impact on LotML.

1. **Make the parity gap data, not comments.** LPython's gaps are commented-out `RUN` lines: 188
   rotted silently, including all classes, generics and interop, and nobody could see how far
   "post sync" had set them back. Instead, each parity program declares the targets it must pass;
   a program failing on LLVM goes on an explicit expected-gap list with a reason; CI fails when a
   listed gap starts passing, so the list only shrinks, and prints the gap count. Supports
   adr:0025 ("that gap is the parity work, measured by the parity suite"). No conflict.
2. **One source of truth per builtin semantic, and test folding against it.** LPython has up to
   three implementations of a builtin (the C++ folder `python_comptime_eval.h`, source code in
   `lpython_builtin.py`, per-backend codegen), and they already disagree. The folder rounds
   `round(-2.7)` to −3 (`python_comptime_eval.h:450-455`); the runtime body truncates to −2, sees
   `f = 0.7 > 0.5` and returns `i + 1 = −1` (`lpython_builtin.py:280-296`). If `lotml-ir` gains
   constant folding, fold only operations whose run-time meaning has one definition, and run the
   parity suite with folding on and off. Follows adr:0020's "a rule is lowered once". No conflict.
3. **An IR walker (`visit.rs`) instead of per-pass traversal.** Hand-written traversal is where a
   missed operand becomes a missing `Dec`. Adding `operands_mut`, `walk_block` and
   `walk_block_mut` beside `Expr::operands` (`ir.rs:736`) gives what ASDL generates for LPython,
   without a generator. ADR 0020: no conflict.
4. **Instrument the IR pipeline.** Turn `native()` into a named pass list. Add an env or `dev`
   switch that writes `text.rs` output after every pass and per-pass timing, as
   `--dump-all-passes` and `--time-report` do. This is cheap, and it lets agents see the IR at the
   point a pass broke it. No conflict.
5. **Prune unreachable functions before emitting LLVM IR.** First measure whether `lotml-llvm`
   emits unreachable functions, such as unused prelude code. If it does, a reachability pass
   rooted at `main`, test blocks and adr:0024 exports cuts `clang` time. No conflict
   (adr:0021/0022).
6. **A Python-side loader for `lotml build --shared`**: a small ctypes module generated next to
   the library, the counterpart of the `.pyi` adr:0012 writes, so CPython can call a hot LotML
   module natively. `@lpython` shows the user experience and its pitfalls (gcc only, no Windows,
   distutils, per-process cache). **Flag**: adr:0012 requires Python→LotML calls to cross a
   *checked, copying* boundary, and the loader must keep those checks. adr:0025 is untouched:
   CPython loads native code, not the other way round.
7. **Struct and array marshalling over ctypes for C interfaces** (Reusable, row 1).
   **Conflicts with adr:0013**, which rejects pointers, structs, arrays and callbacks until an
   ownership rule exists. It needs a superseding ADR first. Recorded only as ready material.

Explicitly not to adopt: deep copy on every container assignment (adr:0003/0008 chose counting
plus reuse to avoid that cost); closures through module globals (not reentrant); linking
`libpython` at build time (rejected by adr:0023, native CPython removed by adr:0025); an
in-process LLVM JIT (adr:0021/0025); sharing the IR with another language's compiler
(Pitfalls §1).

## Pitfalls seen

1. **Sharing an IR and passes with another language's compiler.** libasr is LFortran's, pulled in
   as a submodule. After a "sync", 188 of 400 integration tests were commented out, including
   `class_01-06` (`CMakeLists.txt:842-847`), all generics (`:672`, `:797-802`), all `bindpy`
   (`:666-671`) and most `structs_*` (`:695-712`); generic instantiation is commented out in the
   front end (`python_ast_to_asr.cpp:1206-1239`). The schema drifted Fortran-ward (file I/O,
   OpenMP, array layouts, `.false.` in dumps), and Python programs pay for Fortran passes (`where`,
   `forall`, `implied_do_loops`, `openmp`). LotML's adr:0020 keeps one IR owned by one language.
   Keep it that way.
2. **Each backend reads a different, implicit IR subset.** `_c_skip_passes` lets the C backend
   implement printing, loops, `select` and list expressions itself. That is a second
   implementation of each, and the C backend trails LLVM by 55 "renable c" tests.
3. **The same container differs per backend.** The list struct field order differs between C and
   LLVM. Dicts are hash tables on LLVM but a linear scan for non-integer keys on C. Sharing
   `lotml-runtime` between targets is what prevents this.
4. **Weak hashing.** An integer key hashes to `key % capacity`, and strings use a polynomial
   mod 100000009. Clustered keys degrade lookups, and dict order is not Python's. LotML's
   CPython-exact hash and insertion order are the right call.
5. **Value semantics paid by copying.** Every list, dict, tuple or struct assignment and every
   append deep-copies. Freeing is tied to scope end, not last use.
6. **The compile-time folder and the runtime diverge.** See `round` in Ideas #2. This is a parity
   bug that no assert-only test caught.
7. **Fixed-width defaults that CPython mode does not emulate.** Integer literals are `i32` and
   `int` is refused, yet `lpython.i32(x)` is `int(x)`, so "CPython mode" never wraps or traps
   while native code uses fixed widths. I found no overflow detection in `src/lpython` (a search
   for "overflow" finds only URLs in comments). LotML's overflow trap must stay a rule both
   targets implement.
8. **Assert-only conformance.** Output is never compared, so formatting drifts silently.
   `_lfortran_float_to_str4` uses `"%f"`. LotML's output diff is the stronger contract. Keep it.
9. **Caching without invalidation** was disabled rather than fixed (`python_ast_to_asr.cpp:146-149`).
10. **In-process LLVM.** Version `#if`s throughout the backend, the CPU hard-coded to
    `"generic"`, an 824 KB single-file emitter, and debug line tables that need `llvm-dwarfdump`
    and Python at build time (`lpython.cpp:2264-2287`). LotML's textual IR through `clang`
    (adr:0021) avoids all four.
11. **Hard-coded toolchain.** The C backend always calls `gcc` (`lpython.cpp:1583`), `-lpython3.10`
    and `$CONDA_PREFIX` are baked in (`:1541-1542`), and `system()` builds command lines with
    unquoted paths (`:1503-1507`, `:1527-1535`).
12. **No parse or semantic error recovery.** The first error throws (`parser.cpp:20-31`,
    `python_ast_to_asr.cpp:9112-9121`). That is unusable for LotML's edit loop, and LotML's
    tolerant parser already avoids it.
13. **Giant golden dumps.** 455 ASR S-expression references make any IR change a mass reference
    update. LotML's IR text tests (R1.5) should stay few and focused on one pass each.
