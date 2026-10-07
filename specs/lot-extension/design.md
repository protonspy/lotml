# Lot extension — design

## What changes

Serves R1.1–R1.4, R2.1–R2.5, R3.1, R3.2.

**One predicate for a source file** (R1.1). `files::is_source` is true for the extensions `lot`
and `lotml`, and it replaces each comparison with `"lotml"`: the directory walk under
`files::expand`, which the commands, the MCP server's refresh and the language server's folder
scan all use, and the language server's reload of a changed file. A file named explicitly on the
command line stays accepted whatever its extension, as it is today. Outside the Rust crates, the
Python runtime keeps a traceback's frames by the source file's suffix, and the tree-sitter
grammar lists its file types: both take `lot` beside `lotml`.

**Ambiguity is refused where a command starts** (R1.2). `files::expand` checks what it found for
two paths that differ only in `.lot` against `.lotml`, and fails naming both. Since LotML files do
not import each other, the two would not clash in `check`. They would clash in `build`, which
writes both as `<stem>_lotml.py`, and in a reader's head. The MCP server and the language server
call `files::sources`, the same search without the check. Both servers swallow a failed search
into an empty workspace, so the check there would hide every file because of one stray pair.
There, each file is served on its own, as any two files are.

**Words** (R1.3, R1.4, R3.1). The CLI's `about` becomes "The LotML compiler". Its help, the MCP tools'
descriptions, the `no … file at` messages and `reference/lotml.md` name `.lot` and `.lotml`. The
wiki's page on the semantic compiler states what the language server loads and what `init`
writes, so its two lines on that are brought up to date. The generated Python
module's header comment says to edit the LotML source. The MCP server's atomic write appends
`.partial` to the file's whole name rather than swapping its extension.

**`lotml init`** (R2.1, R2.2). `GUIDE_FILE` becomes `lotml.guide.lot`, and the compiled-in text
moves to `init/lotml.guide.lot`. Before writing the guide, `init` removes a `lotml.guide.lotml`
from the project and reports `lotml.guide.lotml: renamed to lotml.guide.lot`. `init` already
replaces that file's whole text on every run, so the removal loses nothing `init` would have kept.
Leaving it would put two copies of the guide side by side, an R1.2 pair in all but name. The
`AGENTS.md` block names `lotml.guide.lot` and the two extensions.

**The agent harness** (R2.3–R2.5). `safe.checked_name` is the harness's gate on every file name
a model or a trace supplies, and it takes `.lot` beside `.lotml`. The run's snapshots around each
`check` and `test` glob both, or a `.lot` workspace would leave its traces empty. HumanEval's and
MBPP's tasks are posed in `solution.lot`, their prompt naming it. The eight agent-bench tasks have
their files renamed to `.lot`, along with the name in `task.toml` and `prompt.md`. The guide's
seeded programs, the scratch copies its records and evaluation lay, and the phase 1 failures the
evaluation asks about all become `.lot`. The phase 0 and phase 1 experiments keep `.lotml`, since
their committed answers were produced from those prompts. The language's prose name in the agent
harness's prompts stays as it is: changing it changes what a model reads, and no requirement asks
for that.

**The README** (R3.1, R3.2). The title, the badge and the prose say LotML and `.lot`. One sentence
gives the name's origin.

## Risks

- The guidance records in `harness/cache/guide/records/2026-10-06` and the guide trained from them
  show `solution.lotml`. A guide trained before this change and asked about `.lot` files sees a
  path it was not trained on. Rebuilding them belongs to plans/harness-guide.md, and the session
  running it has been told.
