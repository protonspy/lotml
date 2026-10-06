# Agent guide — tasks

## 1 · The texts

- [ ] 1.1 (Unit) Write `lotml.guide.lotml`, every reference section and Not Python rule as code with `test` blocks, and a compiler test that checks it clean and runs its tests green — R1.5
- [ ] 1.2 (Unit) Write the `AGENTS.md` block: the edit loop, the MCP tools by question, `lotml lsp`, the guide; at most 80 lines, held by a test — R1.6

## 2 · `lotml init`

- [ ] 2.1 (Unit) Add `lotml init`: write the guide and the `AGENTS.md` block, replacing an existing block alone, reporting created, updated or unchanged — R1.1, R1.2, R1.3, R1.4
  _Depends 1.1, 1.2_
- [ ] 2.2 (TDD) Register the MCP server in a JSON configuration by text insertion, keeping the other entries and their order, leaving a file that has `lotml` or is not JSON untouched — R2.8, R2.9, R2.10
- [ ] 2.3 (Unit) Set up Claude Code, Codex and Cursor: `.mcp.json` and the `CLAUDE.md` import block, `.codex/config.toml`, `.cursor/mcp.json` — R2.5, R2.6, R2.7, R2.9
  _Depends 2.1, 2.2_
- [ ] 2.4 (Unit) Detect the harnesses, take `--harness` and `--yes`, and show the checklist on a terminal with the detected ones checked — R2.1, R2.2, R2.3, R2.4
  _Depends 2.3_
