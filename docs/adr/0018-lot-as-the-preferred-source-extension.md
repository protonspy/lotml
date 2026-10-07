---
status: accepted
---

# 0018 · `.lot` as the preferred source extension, `.lotml` still accepted

## Context

The language was named lotml on 2026-10-05, with `.lotml` for its files, after the study had used
the placeholder X (`.x`). On 2026-10-07 the user set the project's name as LotML, Lot from Lotus and
ML from Machine Learning, and asked for `.lot` to be supported beside `.lotml`, preferably `.lot`.

By then `.lotml` was in use: 68 files in this repository, every project `lotml init` had set up
with its `lotml.guide.lotml`, and every prompt of the agent harness, the phase 1 gate and the
guide's training records. An extension is hard to take back once files outside the repository
carry it, and dropping one breaks those files.

## Decision

The compiler accepts both `.lot` and `.lotml` as LotML source, everywhere it looks for source
files. `.lot` is preferred: everything written from now on uses it, including what `lotml init`
puts in a project, the agent harness's tasks and new tests. `.lotml` files already written stay as
they are. Two files that differ only in `.lot` against `.lotml`, side by side, are refused by the
commands, since `build` would write both to one module. The command, the crates, the Python
packages, the `<stem>_lotml.py` modules and the `.lotmli` interfaces keep the name `lotml`. LotML
is the name in titles and prose. The other options were these:

- **Rename to `.lot` and drop `.lotml`.** This breaks every project already set up, and the
  committed experiments whose prompts named `.lotml`.
- **Rename every `.lotml` file in the repository.** The churn buys nothing the acceptance of both
  does not, and it would rewrite the fixtures that record what the phase 1 gate's models were shown.
- **Rename the binary and the crates to match.** Every harness registration `lotml init` wrote names
  the `lotml` command, so renaming it breaks them.

## Consequences

- One predicate, `files::is_source`, decides what a source file is. A third extension would be
  added there.
- `lotml init` replaces a project's `lotml.guide.lotml` with `lotml.guide.lot`, so a project
  carries one guide.
- The guide's records and the model trained from them show `.lotml` paths until they are built
  again from the harness's `.lot` tasks (plans/harness-guide.md).
- Results from before this change name `.lotml` files and are not rewritten. The phase 0 and phase 1
  experiments keep `.lotml`, so a rerun asks what was asked before.
