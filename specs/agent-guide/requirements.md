---
autonomy: auto
ci: wait
branch: feat/agent-harness
delivery: in-progress
---

# Agent guide — requirements

## Purpose

lotml is written by coding agents, and an agent starts from what its harness hands it: the
`AGENTS.md` it reads on entry and the tools it is configured with. `lotml init` sets a project up
for them — a lotml block in `AGENTS.md` that teaches the compiler-driven edit loop, a guide written
in lotml itself that the agent reads as code, and the compiler's MCP server registered in each
assistant harness the project uses (Claude Code, Codex, Cursor). The agent harness spec
(specs/agent-harness/) measures whether it helps.

## R1 · The guide and AGENTS.md

- **R1.1** When `lotml init` runs in a directory, the init command shall write the lotml block to `AGENTS.md` there, creating the file when it is absent.
- **R1.2** If `AGENTS.md` already holds a lotml block, then the init command shall replace that block alone and leave every other line of the file as it was.
- **R1.3** When `lotml init` runs in a directory, the init command shall write `lotml.guide.lotml` at its root.
- **R1.4** The init command shall report each file it touches as created, updated or unchanged, and leave a file whose content would not change byte for byte as it was.
- **R1.5** The guide shall show, as lotml code, every section of the language reference and every rule the reference marks Not Python, and shall check with no diagnostics and pass its `test` blocks.
- **R1.6** The lotml block shall tell an agent the edit loop — `check` after every edit, its fixes, `explain`, `test` — which MCP tool answers which question, that `lotml lsp` serves editors, and where the guide is, in at most 80 lines.

## R2 · Assistant harnesses

- **R2.1** The init command shall detect Claude Code, Codex and Cursor by a `.claude/`, `.codex/` or `.cursor/` directory at the root.
- **R2.2** While standard input is a terminal and neither `--harness` nor `--yes` is given, the init command shall show a checklist of the three harnesses with the detected ones checked, let the user toggle them by number, and set up the ones checked when the user accepts.
- **R2.3** Where `--harness` is given, the init command shall set up exactly the harnesses it names, `none` naming no harness, without a checklist.
- **R2.4** While standard input is not a terminal, or where `--yes` is given, the init command shall set up the detected harnesses without a checklist.
- **R2.5** Where Claude Code is set up, the init command shall register the `lotml` MCP server in `.mcp.json` and add a lotml block importing `AGENTS.md` to `CLAUDE.md`.
- **R2.6** Where Codex is set up, the init command shall register the `lotml` MCP server in `.codex/config.toml`.
- **R2.7** Where Cursor is set up, the init command shall register the `lotml` MCP server in `.cursor/mcp.json`.
- **R2.8** When the init command registers the server in an existing configuration, the init command shall keep its other entries and their order.
- **R2.9** If a configuration already declares a server named `lotml`, then the init command shall leave that file unchanged.
- **R2.10** If an existing `.mcp.json` or `.cursor/mcp.json` is not valid JSON, then the init command shall leave it unchanged, name it, set up the rest, and exit with status 2.

## Out of scope

- Language-server configuration for an editor: the block names `lotml lsp`, and each editor is
  wired by its own extension.
- Harnesses other than Claude Code, Codex and Cursor, and user-level configurations outside the
  project.
