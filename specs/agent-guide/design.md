# Agent guide — design

## What changes

Serves R1.1–R1.6, R2.1–R2.10.

A new subcommand, `lotml init [--harness <list>] [--yes] [DIR]`, in a new module
`compiler/crates/lotml/src/init.rs`, with its two texts beside it and compiled in with
`include_str!`: `init/AGENTS.md` (the block's body) and `init/lotml.guide.lotml` (the guide). The
binary carries them, so the files an agent reads always describe the compiler that wrote them, and
running `lotml init` again after an upgrade refreshes both.

**Blocks.** The block in `AGENTS.md` and in `CLAUDE.md` sits between `<!-- lotml:begin -->` and
`<!-- lotml:end -->`. Present, its lines are replaced; absent, it is appended after one blank line.
`CLAUDE.md`'s block is a single `@AGENTS.md` line, Claude Code's import, since Claude Code reads
`CLAUDE.md` and not `AGENTS.md`; Codex and Cursor read `AGENTS.md` themselves.

**The guide** is one lotml file, section by section in the reference's order (`reference/lotml.md`),
each a short comment and code that checks, with a `test` block asserting what the reference says it
does — so `lotml test lotml.guide.lotml` proves the guide, and the compiler's own suite runs it. It
is named so it never takes the place of a program's `main.lotml`; `lotml check .` and `lotml test .`
include it, which costs a project a few milliseconds and tells it if a compiler upgrade changed a
rule.

**Harnesses.** Each is a row — name, detection directory, configuration file, how to register:

| harness | detected by | configuration | entry |
|---|---|---|---|
| Claude Code | `.claude/` | `.mcp.json` | `mcpServers.lotml` |
| Codex | `.codex/` | `.codex/config.toml` | `[mcp_servers.lotml]` |
| Cursor | `.cursor/` | `.cursor/mcp.json` | `mcpServers.lotml` |

The entry is `command = "lotml"`, `args = ["mcp", "--root", "."]`: `lotml` on `PATH`, since the file
is committed and shared, never this machine's absolute path.

**Registering without reordering.** serde_json's map sorts keys unless `preserve_order` is on, and
turning it on for this crate turns it on for the workspace, changing the order of every JSON the
compiler already writes. So a JSON configuration is parsed only to decide (valid? `lotml` already
there?), and written by inserting text: after the `{` of the top-level `"mcpServers"` object, or a
new `"mcpServers"` member after the top-level `{`, at the file's indentation. The result is parsed
again and must hold the entry, or nothing is written. TOML is not parsed: a line
`[mcp_servers.lotml]` means it is there, otherwise the table is appended.

**Checklist.** Without `--harness` or `--yes` and with a terminal on standard input
(`std::io::IsTerminal`), it prints the three rows with `[x]` on the detected ones and reads lines:
numbers toggle, an empty line accepts. No terminal library: a line prompt works in every terminal
and over a pipe the checklist never shows.

**Exit status** follows the binary's: 0 done, 2 when a configuration was left unreadable (R2.10).

## Alternatives considered

- `main.lotml` as the guide's name, the first proposal: it is where a program's `fn main()` lives,
  so `init` in an existing project would collide with it. `lotml.guide.lotml` was chosen with the
  user.
- A guide in Markdown: the reference already is one. A guide in lotml is read by an agent as the
  code it is about to write, and it can be checked and tested, so it cannot drift from the
  compiler.
- Language-server configuration per harness: Claude Code takes a language server only through a
  plugin, and editors through their extensions; the MCP server is what every harness takes from a
  project file.

## Risks

- A harness may change where it reads project MCP configuration; each row is one line of
  `init.rs` to change.
- The guide's tests run on every `lotml test .` of a project; if that becomes noise, a flag can
  skip `*.guide.lotml`, measured first.
