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
- n-0023 2026-10-06 #ceiling @compiler/crates/lotml-py/src/emit.rs — an `inout` argument and a plain read of the same place in one call alias each other on the Python target; the checker rejects only two `inout`s of one place (E0307)
- n-0024 2026-10-06 #ceiling @compiler/crates/lotml-py/src/emit.rs — `inout self` is the object itself, not a box: a method assigning `self = …` does not reach the caller
- n-0025 2026-10-06 #ceiling @compiler/crates/lotml-py/src/emit.rs — `&obj.f` and `&xs[i]` evaluate `obj` and `i` again for the write-back; a place with a call in it runs that call twice
- n-0026 2026-10-06 #gotcha @compiler/crates/lotml-py/src/emit.rs — `:=` is not allowed in a comprehension's iterable or a default value; integer traps there go through `__rt.check`, the slower helper
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
- n-0037 2026-10-06 #ceiling @compiler/crates/lotml-py/runtime/lotml_bind.py — lotml bind binds module-level functions only: Python classes, callables and unions are listed as not bound; binding a class needs an opaque handle type
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
- n-0053 2026-10-06 #gotcha @compiler/crates/lotml-syntax/src/strings.rs — an f-string spec with a nested field, f"{x:{w}}", is parsed as the literal text {w}: both targets stop with ValueError Invalid format specifier at run time
- n-0054 2026-10-06 #ceiling @compiler/crates/lotml-c/runtime/lotml_text.c — the C runtime's character classes and case mapping cover ASCII, Latin-1, Latin Extended-A, Greek and Cyrillic; other scripts are uncased letters, so isalpha/upper there can differ from CPython
- n-0055 2026-10-06 #gotcha @compiler/crates/lotml-py/src/emit.rs — on the Python target, assigning self inside an inout self method (self = T.new()) does not reach the caller; the C target writes it through the pointer, so a program doing it differs between targets
- n-0056 2026-10-06 #ceiling @compiler/crates/lotml-c/src/lower.rs — C target: a trait method taking inout self, generic itself, or naming Self beyond its receiver has no vtable slot, so calling it through dyn is E0402; a typed slot per such method lifts it
- n-0057 2026-10-06 #ceiling @compiler/crates/lotml-c/runtime/lotml_list.c — C target: hash() of a record holding a list, dict or set stops with TypeError, where the Python target hashes that field as a tuple; a hash per collection type that matches _hashable lifts it
- n-0058 2026-10-06 #gotcha @compiler/crates/lotml-py/runtime/lotml_rt.py — Python target: a parallel task returning a value it captured hands back the same object, so changing that result through a var changes the captured binding too; the C target copies on write
