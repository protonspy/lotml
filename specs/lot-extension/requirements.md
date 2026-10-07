---
autonomy: auto
ci: wait
branch: feat/lot-extension
delivery: in-review
pr: 25
---

# Lot extension — requirements

## Purpose

The project is LotML: Lot from Lotus, ML from Machine Learning. Its source files take the shorter
`.lot`, and `.lotml` stays accepted so the files already written keep working. Everything written
from now on uses `.lot`: what `lotml init` sets up in a project, and the files the agent harness
lays for a model. The command, the crates and the packages keep the name `lotml`
(adr:0018-lot-as-the-preferred-source-extension).

## R1 · Source files

- **R1.1** The compiler shall treat a file whose name ends in `.lot` or in `.lotml` as a LotML source file, in every command, in the MCP server, in the language server, in the runtime's tracebacks and in the tree-sitter grammar's file types.
- **R1.2** If a directory searched for source files holds `x.lot` and `x.lotml` side by side, then the compiler's commands shall refuse to run and name both files.
- **R1.3** The compiler's help, its MCP tools' descriptions and its messages shall name both extensions, `.lot` first, wherever they name the source files.
- **R1.4** The language reference shall name both extensions, `.lot` first.

## R2 · What is written

- **R2.1** The init command shall write the language guide as `lotml.guide.lot`, and the `AGENTS.md` block it writes shall name that file and both extensions.
- **R2.2** If the project holds a `lotml.guide.lotml`, then the init command shall write the guide in its place as `lotml.guide.lot`, remove the old file and report the rename.
- **R2.3** The agent harness shall lay the files it gives a model as `.lot` files: the HumanEval and MBPP tasks' `solution.lot` and the agent bench's workspaces, solutions and hidden tests.
- **R2.4** The agent harness's safe layer and its snapshots of a workspace shall take `.lot` and `.lotml` files alike.
- **R2.5** The guide's seeded failures, its records and its evaluation shall lay their programs as `.lot` files.

## R3 · The name

- **R3.1** The compiler's help shall call the project LotML, keeping `lotml` for the command, the crates and the packages.
- **R3.2** The README shall call the project LotML, say once where the name comes from, Lot from Lotus and ML from Machine Learning, and show `.lot` in its badge and examples.

## Out of scope

- Renaming the `.lotml` files already in the repository, its test fixtures among them, except
  the agent bench's, which are given to models (R2.3).
- Renaming the binary, the crates, the Python packages, the `<stem>_lotml.py` modules `build`
  writes, or the `.lotmli` interfaces.
- The phase 0 and phase 1 experiments' prompts and committed results, and `research/` with its
  X and `.x`: they record what the models saw.
- Rewriting the existing ADRs, wiki pages and specs to say LotML.
- Rebuilding the guide's records and training it again on `.lot` paths: plans/harness-guide.md.
