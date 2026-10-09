# Notes

Every small durable observation about this project, one per line. The gotcha, the
why-not, the "careful, this looks wrong and is not" — the things that used to be a
comment beside the code, where only the reader already looking at that line ever
found them.

**Notes do not live in the code.** A comment says what a thing is and how to use
it; anything else is a note, and it belongs here with the path it is about
attached to it.

One note is one line, index fields first:

```markdown
- n-0000 2026-02-09 #gotcha @internal/cli/launch.go — wrap writes MCP config to the agent's own file, so it outlives the session
```

`n-0000` is the id; real ones start at `n-0001` and are never reused, so a note can
be cited as `n-0042` from anywhere — including the one place code may still mention
one. Then the date. Then `#tags`, at least one, which is what the log is queried by.
Then `@paths`, repo-relative, which is what keeps a note attached to the code without
living inside it. Then an em dash, and the note itself.

The one line is the contract: a match is a whole note, never a fragment of one, so
`grep ' #gotcha ' docs/notes.md` and `scc notes find --tag gotcha` answer the same
question. Write with `scc notes add "…" --tag gotcha --path <path>`, which
allocates the id and gets the format right; read with `scc notes find`,
`scc notes tags`, `scc notes show n-0001`.

**If it needs a second line, it is not a note.** Something learned and worth
explaining is a `wiki/` page. A decision that is hard to reverse is an `adr/`
record. Something that has to be done is a task in a plan. A note is the thing
none of those three would take.

<!-- Delete this guidance once the log below reads for itself: until then a grep
over this file answers with the example above as well as with the notes. -->

## Log

<!-- Notes go below, oldest first. `scc notes add` appends here. -->
- n-0001 2026-10-05 #gotcha @research/tokens/corpus — ruff format rewrites the paired corpus, which is measured byte for byte; research/ruff.toml excludes it
- n-0002 2026-10-05 #gotcha @research/tokens/counter.py — the Llama, Gemma and Mistral tokenizers add a BOS token by default; count with add_special_tokens=False, pinned by test_counter.py
- n-0003 2026-10-05 #gotcha @research/tokens/counter.py — the first run of research/tokens downloads the tokenizers from Hugging Face and needs network access
- n-0004 2026-10-05 #gotcha @research/pilot/check.py — the ? in T? is the named terminal QMARK so the tree can tell int from int?; as an anonymous token, Lark drops it
- n-0005 2026-10-05 #ceiling @research/pilot/semantics.py — semantics.py infers types only from annotations and literals; a real lotml type checker replaces it
- n-0006 2026-10-05 #decision @research — build and test gates skipped: the repository only holds research code; record them with scc check set once the compiler exists
- n-0007 2026-10-05 #decision @docs — every artifact is written in English: docs, wiki slugs, ADR filenames, plans, code, commit messages and PR bodies
- n-0008 2026-10-05 #gotcha @research/experiments/indentation/slips.py — Lark's Indenter never sees the first code line's indentation, which Python rejects; classify() treats an indented first line as a syntax error
- n-0009 2026-10-05 #gotcha @research/experiments/grammar/grammars.py — llguidance's Lark dialect refuses terminal priorities and %declare, and with %ignore for spaces an exact-indentation newline terminal still accepts deeper lines: indentation grammars must match whole lines
- n-0010 2026-10-05 #gotcha @research/experiments/transpiler/lotml_rt.py — dict views cannot be deep-copied, so under value semantics keys(), values() and items() return lists, a snapshot
- n-0011 2026-10-05 #gotcha @research/experiments/transpiler/transpile.py — the pilot grammar drops anonymous tokens, so True/False and the comparison operators vanish from the tree; transpile.parser() names them
- n-0012 2026-10-05 #gotcha @research/literature/fetch.py — the first fetch downloads docling's layout models and converts at about 1.5 s per page on CPU; arXiv asks for 3 s between downloads
- n-0013 2026-10-05 #gotcha @research/experiments/editing/editing.py — a SEARCH block must match at a line start: a bare substring match lets a less-indented search apply inside a deeper line
- n-0014 2026-10-05 #ceiling @research/experiments/transpiler/transpile.py — the research transpiler copies on every read-and-bind, not only into var/inout as lotml's compiler will; fine for tests, not for performance
- n-0015 2026-10-05 #ceiling @research/experiments/transpiler/transpile.py — the step budget counts Python line events, so work done inside one line (sum(range(10**9)), a huge allocation) escapes it; a subprocess with a wall-clock limit is the real guard
- n-0016 2026-10-05 #gotcha @harness/lotml_harness/tasks/sources.py — MultiPL-E's license forbids its contents as training data: the HumanEval and MBPP tasks are for evaluation only, never for the Python-to-lotml corpus
- n-0017 2026-10-05 #decision @harness/lotml_harness/tasks/build.py — a task whose canonical Python fails under the typed signature is refused, not fixed: Python returns 240.0 where the type says int, and lotml would reject that program
- n-0018 2026-10-05 #ceiling @harness/lotml_harness/lang/transpile.py — the executor erases types: int + f64 and other static type errors run as Python would, so pass@1 counts programs the compiler would reject; lifts with the phase 1 type checker
- n-0019 2026-10-05 #ceiling @harness/lotml_harness/lang/transpile.py — lambdas capture variables by reference in the executor, not by copy as lotml specifies; a lambda reading a var reassigned later sees the new value
- n-0020 2026-10-05 #gotcha @harness/lotml_harness/lang/transpile.py — inout arguments are boxed and written back with a walrus after the call; an &x inside a lambda cannot write back to the enclosing x
- n-0021 2026-10-05 #gotcha @harness/lotml_harness/lang/dialects.py — llguidance's lexer decides a lexeme one byte ahead: a lexeme that can run on into the next line (a multi-line end of line, a fused 'is not') rejects valid code
- n-0022 2026-10-06 #ceiling @compiler/crates/lotml-fmt/src/lib.rs — the formatter joins every statement onto one line and never wraps; a long call stays long until line breaking is designed
<!-- n-0023 removed -->
<!-- n-0024 removed -->
<!-- n-0025 removed -->
<!-- n-0026 removed -->
- n-0027 2026-10-06 #ceiling @compiler/crates/lotml-check/src/body.rs — E0308 fires only for a `var` copy of a parameter changed in a function returning nothing, and for `var self` not returned; other dropped changes go unreported
- n-0028 2026-10-06 #ceiling @compiler/crates/lotml-check/src/prefix.rs — `check --prefix` holds every E0205, since a diagnostic does not say whether the receiver is a user type an `impl` below could extend
- n-0029 2026-10-06 #gotcha @compiler/crates/lotml-py/src/lib.rs — on Windows `python.exe` in WindowsApps only opens the Store and prints nothing to `--version`; discovery requires a version on stdout and falls back to `py -3`
- n-0030 2026-10-06 #decision @compiler/crates/lotml-py/runtime/lotml_rt.py — `T ! PyError` at the boundary is `lotml_rt.python(f, …)`; v1 calls into no foreign Python, since `math` is lotml's own module and its domain errors stop the program like division by zero
- n-0031 2026-10-06 #gotcha @compiler/crates/lotml-check/src/body.rs — `var xs = [str]` — a type written where an empty list goes — reports `fn() -> {unknown}`; a diagnostic naming the habit, with the fix `var xs: [str] = []`, would repair in one round (seen in phase 1, Qwen)
- n-0032 2026-10-06 #gotcha @compiler/crates/lotml-check/src/body.rs — `var s = xs.sort()` binds nothing: the error surfaces later as `a None cannot be sliced`; reporting at the binding with `sorted(xs)` as the fix would point at the cause (seen in phase 1, Qwen)
- n-0033 2026-10-06 #gotcha @compiler/crates/lotml-syntax/src/parser.rs — a nested `fn` inside a body cascades into E0003 and E0201 `fn`; a dedicated diagnostic saying functions live at the top level would repair in one round (seen in phase 1, Qwen)
- n-0034 2026-10-06 #ceiling @compiler/crates/lotml/src/mcp.rs — the MCP server stats every .lotml file under the root before each call; a file watcher if projects grow to thousands of files
- n-0035 2026-10-06 #ceiling @compiler/crates/lotml-ide/src/symbols.rs — a method called through a bounded type parameter (`i.show()` with `T: Show`) resolves to no symbol, so references and rename miss it; resolve it to the bound trait's method
- n-0036 2026-10-06 #ceiling @compiler/crates/lotml-py/runtime/lotml_rt.py — a record's methods reach Python unwrapped: arguments unchecked and a failure returned as Err, unlike top-level functions; wrap them as export does
- n-0037 2026-10-06 #ceiling @compiler/crates/lotml-bind/src/binder.rs — lotml bind binds module-level functions only: Python classes, callables and unions are listed as not bound; binding a class needs an opaque handle type
- n-0038 2026-10-06 #gotcha @compiler/crates/lotml/src/exec.rs — Python's text-mode stdout writes \r\n on Windows: output a Rust command reads from Python is normalized before it is written to a file
- n-0039 2026-10-06 #ceiling @compiler/crates/lotml-py/runtime/lotml_rt.py — parallel runs each task on an OS thread, at most 256 at once: tasks beyond that wait for a thread, and a blocking call holds one; green threads arrive with the C target
- n-0040 2026-10-06 #ceiling @compiler/crates/lotml-check/src/interface.rs — a C interface passes scalars and read-only strings only: pointers, structs, arrays and callbacks need an ownership rule first
- n-0041 2026-10-06 #gotcha @harness/lotml_harness/lang/dialects.py — Lark settles shift/reduce conflicts by shifting without a word; a grammar generated for another engine must say it (prec.right in tree-sitter)
- n-0042 2026-10-06 #gotcha @compiler/crates/lotml/src/files.rs — a git hook exports GIT_DIR, and a git command a test or the compiler runs inherits it: clear the GIT_* location variables, or it writes into the repository being pushed
- n-0043 2026-10-06 #gotcha @compiler/crates/lotml-py/runtime/lotml_rt.py — a str passed to C is refused if it holds a NUL: C would read only up to it, so what it saw would not be what was checked
- n-0044 2026-10-06 #gotcha @compiler/crates/lotml/src/exec.rs — lotml bind refuses Windows device names (con, nul, comN, lptN…): nul.lotmli would be written nowhere
- n-0045 2026-10-06 #gotcha @compiler/crates/lotml/src/files.rs — files::walk skips symlinked directories: a link can lead out of the project or round in a loop; interfaces_for stops at the directory holding .git
- n-0046 2026-10-06 #gotcha @harness/lotml_harness/experiments/awaits.py — awaits experiment runs model answers with limit_memory and cwd in a fresh temporary directory, never the repository
- n-0047 2026-10-06 #gotcha @compiler/crates/lotml/src/exec.rs — scripts lotml starts run python -P with the scratch dir first and the working dir last on sys.path, so a planted json.py never shadows the library
- n-0048 2026-10-06 #ceiling @compiler/crates/lotml/src/mcp.rs — MCP unchanged() compares mtimes just before save: an edit inside the filesystem's timestamp tick, or between the check and the rename, is still lost; size plus mtime or a content hash narrows it further
- n-0049 2026-10-06 #ceiling @compiler/crates/lotml/src/exec.rs — wait_limited kills only the python child on the deadline: a grandchild a test spawned can outlive it, holding the pipes; a process group or Windows job object would end the tree
- n-0050 2026-10-06 #ceiling @compiler/crates/lotml/src/files.rs — interfaces_for walks to the filesystem root when no .git is above the file, so a bindings/ planted in a shared ancestor (/tmp, C:\) is read; stop also at home or a fixed depth if it matters
- n-0051 2026-10-06 #ceiling @harness/lotml_harness/experiments/awaits.py — the awaits judge reads its report from the last stdout line of the process running the model's code, so an answer could print a forged report and exit; a report on a separate fd the parent names closes it, as for the older judges
- n-0052 2026-10-06 #gotcha @compiler/crates/lotml/src/exec.rs — lotml bind takes ASCII module names only: Windows reserves COM¹ and LPT¹ too, and every name typeshed binds is ASCII
- n-0053 2026-10-06 #gotcha @compiler/crates/lotml-fmt — lotml fmt puts a blank line before a comment that opens a function body; the guide keeps such comments above the fn
- n-0054 2026-10-06 #gotcha @compiler/crates/lotml-check — a result of None compared with == Ok(None) is E0204 (Ok(None) types as an optional); a test unwraps it with ? instead
- n-0055 2026-10-06 #gotcha @harness/lotml_harness/agent/run.py — ChatOpenRouter's request_timeout is in milliseconds (the SDK's timeout_ms): 180 meant 180 ms, and its retry backoff then ran 300 s before a ReadTimeout
- n-0071 2026-10-06 #gotcha @compiler/crates/lotml-check/src — the checker types a tuple literal by its own elements, even under an annotation: (None, 1) is (_?, int) and never (int?, int?), so hidden blocks compare such results element by element
- n-0072 2026-10-06 #ceiling @harness/lotml_harness/guide/seeded.py — drop-var mutants are mostly dropped as fixable (51 of 64), since check --fix adds the var; the mutability family's share of each program's budget is spent judging them
<!-- n-0073 removed -->
- n-0074 2026-10-06 #ceiling @harness/lotml_harness/guide/seeded.py — on Windows a deadline kill ends the call's tree asynchronously, so its scratch directory can still be in use when removed; seeded's copies ignore cleanup errors and may leave lotml-seeded-* directories in TEMP
- n-0056 2026-10-06 #gotcha @compiler/crates/lotml-syntax/src/strings.rs — an f-string spec with a nested field, f"{x:{w}}", is parsed as the literal text {w}: both targets stop with ValueError Invalid format specifier at run time
- n-0057 2026-10-06 #ceiling @compiler/crates/lotml-runtime/c/lotml_text.c — the C runtime's character classes and case mapping cover ASCII, Latin-1, Latin Extended-A, Greek and Cyrillic; other scripts are uncased letters, so isalpha/upper there can differ from CPython
<!-- n-0058 removed -->
- n-0059 2026-10-06 #ceiling @compiler/crates/lotml-ir/src/lower.rs — C target: a trait method taking inout self, generic itself, or naming Self beyond its receiver has no vtable slot, so calling it through dyn is E0402; a typed slot per such method lifts it
- n-0060 2026-10-06 #ceiling @compiler/crates/lotml-runtime/c/lotml_list.c — C target: hash() of a record holding a list, dict or set stops with TypeError, where the Python target hashes that field as a tuple; a hash per collection type that matches _hashable lifts it
- n-0061 2026-10-06 #gotcha @compiler/crates/lotml-py/runtime/lotml_rt.py — Python target: a parallel task returning a value it captured hands back the same object, so changing that result through a var changes the captured binding too; the C target copies on write
- n-0062 2026-10-06 #ceiling @compiler/crates/lotml-runtime/c/lotml.c — C target: a test that fails, errs or panics leaves by longjmp, so the values its frames held stay allocated until the test program exits; unwinding drops per frame would free them
- n-0063 2026-10-06 #gotcha @compiler/crates/lotml-ir/src/lower.rs — checker: names unpacked from a list, (a, b) = s.split("/"), stay inference variables, so int(a) on a str passes the check; the C backend falls back to the locals' types
- n-0064 2026-10-06 #gotcha @compiler/crates/lotml-runtime/c/lotml_dict.c — C target: a set display of three or more constants iterates as CPython 3.12-3.13 fold it (frozenset rebuilt in table order); CPython 3.14 adds the items in order, so parity tests need LOTML_PYTHON naming 3.12 or 3.13
- n-0065 2026-10-06 #ceiling @compiler/crates/lotml-runtime/c/lotml_text.c — C target: a format width or precision above 10000 stops with ValueError, where CPython takes up to sys.maxsize; layout cost is quadratic in the width, so a single-pass zero-padded grouping would let LT_SPEC_LIMIT rise
- n-0066 2026-10-06 #ceiling @compiler/crates/lotml-runtime/c/lotml.h — C target: a cell count that reaches INT32_MAX (or INT32_MIN + 1 when shared) stays there, so the cell leaks rather than being freed while held; 64-bit counts would lift it
- n-0067 2026-10-06 #gotcha @compiler/crates/lotml-ir/src/lower.rs — C target: Ty is an owned tree, so f((x, x)) inside f[T] doubles the type per instance; a bound on instance count alone built 2^n-node types and exhausted RAM, so instance() also bounds type size
<!-- n-0068 removed -->
- n-0069 2026-10-06 #gotcha @compiler/crates/lotml-ir/src/lower.rs — C target: for x in xs walks a held snapshot, so a body that changes xs in place still sees every element, while the Python target walks the live list and stops early; parity holds only for bodies that leave xs alone
- n-0070 2026-10-06 #ceiling @harness/lotml_harness/experiments/gate2.py — lotml run/test --target c runs the program with no time or memory cap, and the harness's subprocess timeout kills lotml but not its C child; a job object or rlimit on the child, and a process-group kill in the harness, lift it
- n-0075 2026-10-07 #gotcha @compiler/crates/lotml/src/guide/client.rs — serde_json sorts object keys and llama.cpp compiles a JSON schema to a grammar in the order its properties come; the guide's answer schema is sent as text to keep the trained order
- n-0076 2026-10-07 #gotcha @specs/python-bridge/requirements.md — python-bridge: an embedded CPython in a distributed binary must run isolated — fixed sys.path, PYTHONPATH, PYTHONHOME and the working directory ignored — or it imports whatever module the environment plants; an interface file types the call, it does not make the imported code trusted
- n-0077 2026-10-07 #gotcha @specs/c-abi-export/requirements.md — c-abi-export: a C caller follows neither LotML's types nor errors as values; the spec decides which types cross, who frees what, what a trap does at the boundary, and that only marked functions are visible
- n-0078 2026-10-07 #gotcha @specs/llvm-backend/requirements.md @compiler/crates/lotml-runtime/c/lotml.h — llvm-backend: lt_panic, lt_overflow and every checked operation take lt_at {file, line, function} by value, 24 bytes; LLVM does not lower aggregates to the C ABI (hidden pointer on Win64, byval on SysV), so what the LLVM backend calls takes aggregates by pointer, lt_at as a pointer to a static constant
<!-- n-0079 removed -->
- n-0080 2026-10-07 #gotcha @specs/llvm-backend/requirements.md @compiler/crates/lotml-llvm/src/emit.rs — llvm-backend: the C target takes a panic's line from #line and __LINE__; LLVM has no preprocessor, so each site's lt_at is a constant built from the line the IR statement carries
<!-- n-0081 removed -->
- n-0082 2026-10-07 #gotcha @compiler/crates/lotml-ir/src/symbol.rs @compiler/crates/lotml-syntax/src/lexer.rs — symbol: the lexer takes any byte >= 0x80 as an identifier character and lotml_ir::symbol puts names into C symbols as they are, so a non-ASCII or bidi-control identifier reaches the generated C (a confusing compiler error, or text a reviewer cannot see); an injective escape in symbol.rs, shared by both native backends, would close it
- n-0083 2026-10-07 #ceiling @compiler/crates/lotml-llvm/src/types.rs — llvm-parity: the per-type functions (inc, dec, eq, cmp, hash, repr, show, share, reuse, own) are IR written by lotml-llvm/src/types.rs, about 1100 lines; a C file generated beside the program and compiled with the runtime is the alternative if it keeps growing or a type needs C's help
- n-0084 2026-10-07 #gotcha @compiler/crates/lotml-runtime/c/lotml.c — lotml-runtime: a helper lotml.h defines is LT_INLINE (C99 inline) and needs its extern declaration in lotml.c, or LLVM IR calling it by name fails to link; abi::signature reads its LLVM type from lotml.h
- n-0085 2026-10-07 #gotcha @compiler/crates/lotml-check/src/body.rs — lower: the checker types a prelude function used as a value Func([], Error), so map(int, xs) and sum over it are left to infer; lower::left_to_infer fills them from key_type, which knows each prelude function's result
- n-0086 2026-10-07 #gotcha @compiler/crates/lotml-llvm/src/driver.rs — c-abi-export: a shared library is never built with LOTML_SANITIZE: ASan's runtime has to be the first library its host loads, and the C program the tests link it into is not built with one
- n-0087 2026-10-07 #ceiling @compiler/crates/lotml-runtime/c/lotml.c — c-abi-export: lt_tasks_running is changed without atomics around a parallel call, which holds within one program; two host threads each calling an exported function that runs parallel at once race on it, and so on whether output is locked; an atomic count lifts it
- n-0088 2026-10-07 #gotcha @compiler/crates/lotml-py/src/from_ir.rs — lotml-py: Python 3.12 refuses a Name, Attribute, Subscript, List, Tuple or Starred node without ctx where 3.13 defaults it, so a node missing one passes on a 3.13 machine and fails CI; the backend test every_node_with_a_context_carries_one guards it
<!-- n-0089 removed -->
<!-- n-0090 removed -->
<!-- n-0091 removed -->
- n-0092 2026-10-07 #ceiling @compiler/crates/lotml-runtime/c/lotml_text.c — lt_offset counts code points from the start, so s[i] is O(n) on non-ASCII text and an index loop over it quadratic
- n-0093 2026-10-07 #ceiling @compiler/crates/lotml-runtime/c/lotml.c — lt_shortest tries up to 17 precisions through snprintf and strtod; Ryu finds the shortest form directly
- n-0094 2026-10-07 #ceiling @compiler/crates/lotml-llvm/src/emit.rs — the emitted IR calls the extern lt_inc rather than the inline one in lotml.h, and nothing optimizes the two units together
- n-0095 2026-10-07 #gotcha @compiler/crates/lotml-llvm/src/driver.rs — the recursion limit differs by target: CPython's 1000 frames under run, the OS stack (1 MiB on Windows) under build
- n-0096 2026-10-07 #ceiling @compiler/crates/lotml-db/src/lib.rs — checked is one query per file with absolute spans, and interfaces is untracked, so an edit re-checks every function and re-parses every interface
- n-0097 2026-10-07 #gotcha @harness/lotml_harness/agent/report.py — wilson() pools every graded run, so several runs of one task count as independent trials and the interval comes out too narrow
- n-0098 2026-10-07 #gotcha @harness/lotml_harness/corpus/pipeline.py — the corpus translates MultiPL-E's typed copies, whose licence forbids training; it is a measurement, never a training seed
- n-0099 2026-10-07 #gotcha @compiler/crates/lotml-check/src/body.rs — the checker keys expression types by span: a debug build asserts no two expressions record one, bar two parse-error nodes at the same missing text, both typed Error
- n-0100 2026-10-07 #gotcha @compiler/crates/lotml/src/exec.rs — lotml run on Windows with stdout piped: the Python target prints in the ANSI code page and panics with UnicodeEncodeError on text outside it, where the native target writes UTF-8 (run removes PYTHONIOENCODING)
- n-0101 2026-10-07 #gotcha @harness/lotml_harness/experiments/parity.py — lotml test --json: a panicking test's trace names every frame on the Python target and only the frame that panicked on the native target; the parity suite compares traces exactly, so a corpus test that panics would differ
- n-0102 2026-10-07 #ceiling @compiler/crates/lotml-py/runtime/lotml_rt.py — the Python target's sum of a u64 list checks its partial sums against i64, where the native lt_sum_u64 allows up to 2**64-1: lotml_rt cannot tell a u64 list from an int one
- n-0103 2026-10-07 #ceiling @compiler/crates/lotml-llvm/src/types.rs — recursion over data runs the generated drop, share, eq, hash and repr functions once per level of a recursive type's value, uncounted: a deep enough value still overflows a native stack where CPython raises its own error
- n-0104 2026-10-08 #ceiling @compiler/crates/lotml-db/src/lib.rs — n-0096's interfaces half is fixed: interfaces() is a tracked query over a durable input (plans/build-and-check-speed.md 2.2); the per-file check with absolute spans remains, specified by specs/incremental-check/
- n-0105 2026-10-08 #ceiling @compiler/crates/lotml-check/src/parts.rs — each item's check keeps its own types, up to the body limit, so a file's memos hold up to that per item rather than the module limit per file; the editor holds only the files it has open
- n-0106 2026-10-08 #ceiling @compiler/crates/lotml-db/src/lib.rs — any change to a file's signatures checks every item again, even one that never reads the signature that changed (specs/incremental-check/R1.5); per-signature dependencies lift it
- n-0107 2026-10-08 #ceiling @compiler/crates/lotml-db/src/lib.rs — n-0096 is fixed: lotml-db checks one item per query with item-relative spans (specs/incremental-check/); what remains is the whole file parsed and its declarations collected on every edit
- n-0108 2026-10-08 #gotcha @harness/tests/test_agent_humaneval_record.py — on Windows the hang case of test_a_failing_test_a_hang_and_an_oversized_output_are_errors fails about half the runs: the killed child still holds lotml-record-* when TemporaryDirectory cleans up (WinError 32)
- n-0109 2026-10-08 #ceiling @compiler/crates/lotml-ir/src/depth.rs — the recursion count doubles fib on the LLVM target (0.124 s to 0.248 s, harness/results/benchmarks-llvm.md): a thread-local load, compare and store around each call of a body of about 1 ns, against R3.1's 5%; the others move within the run-to-run noise of about 8%, mandelbrot and sieve having no counted function at all; passing the count in a register through calls between counted functions lifts it
- n-0110 2026-10-08 #ceiling @compiler/crates/lotml-runtime/c/lotml.c — the 64 MiB stack holds 1,000 counted calls only while a frame and the uncounted calls under it stay under 64 KiB: a recursive function with thousands of live locals at -O0 still overflows the native stack before RecursionError; a stack probe or a frame-size check at emission lifts it
- n-0111 2026-10-08 #ceiling @compiler/crates/lotml-llvm/src/export.rs — an exported library function recurses on its host's thread and stack (R2.3), often 512 KiB to 1 MiB, so frames over about 1 KiB overflow it before the limit of 1,000; a lower limit for exports or a stack check lifts it
- n-0112 2026-10-08 #gotcha @compiler/crates/lotml-llvm/src/cache.rs — a cache entry is marked used through a handle that on Windows may write its attributes alone: one opened for writing its data keeps another build's link.exe, which shares what it reads for reading only, from opening the entry (LNK1104 on a cold cache)
- n-0113 2026-10-08 #ceiling @harness/lotml_harness/experiments/binding_coverage.py — binding coverage counts a stub's public names without following `from x import *` or `__all__ += other.__all__`: a stub that re-exports only that way, as typeshed's os.path does, counts 0 public names; following the star import to the stub it names lifts it
- n-0114 2026-10-08 #ceiling @compiler/crates/lotml-bind/src/binder.rs — n-0037 narrowed: lotml bind now binds a callable, a union, Any and a class as PyObject (specs/python-object), so a function is left out only when overloaded or a coroutine; a class is still opaque, never typed, until specs/python-classes
- n-0115 2026-10-08 #ceiling @compiler/crates/lotml-py/src/resolve.rs — a project .venv is checked for owner and world-writable root on Unix only; Windows refuses a drive root but reads no ACL, so a .venv planted by another user in a shared folder above the project is run
- n-0116 2026-10-08 #ceiling @compiler/crates/lotml/src/files.rs — an interface generated on import is kept for the life of the process, so a long-lived language or MCP server binds a lock's new versions only after a restart; the lock's hashes (bind-on-import R2.2, R3.1) are what lifts it
- n-0117 2026-10-08 #gotcha @compiler/crates/lotml-bind/src/binder.rs — a stub that re-exports from a submodule (certifi's from .core import where, os.path's from posixpath import *) binds no function, so its import fails on import; with classes, it is why 12 of 36 corpus modules fail (harness/results/bind-on-import.md), and no spec covers re-exports yet
- n-0118 2026-10-08 #ceiling @compiler/crates/lotml/src/files.rs — the process-wide memo of generated interfaces also keeps an E0225 mark decided against lotml.lock when it was made, so a long-lived server warns until it restarts after bind --lock; an unbound module is retried after 5 s, so an environment made later heals
- n-0119 2026-10-09 #ceiling @compiler/crates/lotml-bind/src/binder.rs — n-0114 narrowed: lotml bind now binds an overloaded function, constructor or method once per overload (specs/python-overloads), so a function is left out only as a coroutine or with a parameter named by a LotML keyword
- n-0120 2026-10-09 #ceiling @compiler/crates/lotml-check/src/body.rs — an overloaded Python call checks its arguments with no expected type, so a literal meant for an i32 or f32 overload is an int or f64 there and fits no such overload; typing the literals after the choice lifts it
- n-0121 2026-10-09 #ceiling @compiler/crates/lotml-check/src/body.rs — a PyObject parameter the binder widened from a union fits, as the loose fit, an argument the stub's own type refuses, so such a call can take an earlier overload than Python's checker; the boundary turns the wrong result into Err(PyError), and typing unions lifts it
- n-0122 2026-10-09 #ceiling @compiler/crates/lotml-check/src/body.rs — a member a generic Python class inherits from a generic base takes fresh type arguments, since the binder drops a base written with arguments; writing a base's arguments in the class block lifts it
- n-0123 2026-10-09 #ceiling @compiler/crates/lotml-check/src/interface.rs — a Python class whose only constructor is a generic base's has none, since which arguments the subclass fixes is in a base the interface does not write (adr:0036)
- n-0124 2026-10-09 #ceiling @compiler/crates/lotml-bind/src/binder.rs — n-0117 narrowed: a name a stub re-exports is bound from the stub it comes from (specs/python-reexports), so os.path, certifi, idna, charset_normalizer, dateutil.parser and numpy bind; a submodule re-exported as a module (from os import path as path) is still not a name of the module
- n-0125 2026-10-09 #gotcha @compiler/crates/lotml/src/exec.rs — with uv beside lotml but a cache it will not use (on Windows a LOTML_CACHE_DIR outside the user's profile), lotml runs no uv and falls back to python3, python or py -3 without saying so; py.exe in C:\Windows is found even with an empty PATH
- n-0126 2026-10-09 #gotcha @compiler/crates/lotml/src/exec.rs — n-0125 fixed: a uv lotml finds and will not run, its cache refused, is now an error naming the uv and why, after LOTML_PYTHON and the virtual environment, never the path's python (plans/uv-cache-fallback.md)
