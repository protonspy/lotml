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
