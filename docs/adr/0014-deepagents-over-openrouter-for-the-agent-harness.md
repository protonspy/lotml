---
status: accepted
---

# 0014 · deepagents over OpenRouter for the agent harness

## Context

The evaluation harness asks a model for one answer and judges it; the [[evaluation-harness]] page
lists agents with tools editing several files as the measurement still missing, and lotml's
compiler was built as an agent's tool ([[semantic-compiler]]), with an MCP server since phase 2.
Measuring it needs a coding agent loop: a model that plans, reads and edits files, calls the
compiler, and stops when it judges the task done. Writing that loop ourselves means owning
planning, context management and tool dispatch, none of which is what lotml studies. The open
models the first runs use are hosted, since a local run of an agent with dozens of steps per task
takes hours; the harness already reaches OpenRouter for raw completions over the standard
library, but an agent needs chat with tool calls.

## Decision

The agent harness runs a deepagents agent (LangChain's agent harness, built on LangGraph) over a
`ChatOpenRouter` model from `langchain-openrouter`, `z-ai/glm-5.3-flash` first. The agent's files
are a `FilesystemBackend` confined to a scratch copy of the task's workspace, with no shell; the
compiler's tools reach it through a small client of `lotml mcp`. Both packages sit in the harness's
`agent` dependency group, pinned.

## Consequences

- The agent loop is deepagents', so a result measures that loop as much as the model; a second
  framework, or Claude Code driven headless, would be a further arm rather than a replacement.
- LangChain and LangGraph enter the harness's lock file, some sixty packages, all in one group.
- OpenRouter routes a model to one of several providers; a row records the provider when the
  response names it, and pinning one is a constructor argument when results must not mix them.
- The key stays in `OPENROUTER_API_KEY`, never in a file, and never in the environment of the
  compiler process that runs the agent's code.
