---
autonomy: auto
ci: wait
---

# Guide tool — requirements

## Purpose

The harness guide reaches an agent as one more tool of the compiler's MCP server, the server
`lotml init` already registers in Claude Code, Codex and Cursor (specs/agent-guide/), so any
harness and any model can call it (plans/harness-guide.md). The study behind it says how it must
behave: systems that held up showed only what their checker accepted — Lean Copilot, Eiffel-tools,
PyFiXV — and PyFiXV bought its precision with silence; the line a compiler reports is often not the
one to fix; and an agent, not a person, is the reader (docs/wiki/pages/compiler-embedded-model.md).
So the tool says where to change the code and what kind of change, shows an edit only once it
checks, says nothing when it is unsure, and never edits.

## R1 · Configuration

- **R1.1** Where a harness guide configuration is found, in the file `LOTML_HARNESS_GUIDE` names or else in `harness-guide.toml` in the user's lotml configuration directory, the MCP server shall list a `guide` tool and name it in its instructions.
- **R1.2** The MCP server shall read from the configuration the guide server's URL, the model, the confidence threshold, the context budget, the time limit, the renderer version and the records the guide was trained from, resolving a relative path against the configuration's directory.
- **R1.3** If no configuration is found, then the MCP server shall list no `guide` tool and answer every other tool as before.
- **R1.4** If the configuration cannot be read, names a URL that is not plain HTTP on the loopback interface, or names a renderer version other than the compiler's, then the MCP server shall list no `guide` tool and say why on standard error.

## R2 · The call

- **R2.1** When the agent calls `guide`, the guide tool shall check the files it names, or all of the project's, and take as the state the first file with errors and its diagnostics, or else the first failing test block with the values each side of its comparison had.
- **R2.2** If the files check and every test block passes, then the guide tool shall answer that there is nothing to guide without asking the guide server.
- **R2.3** The guide tool shall render the guide's system message and the state — the task when the agent gives one, the file with its lines numbered, the diagnostics or the failing block — with one renderer, which the guide command also prints for a state given as JSON.
- **R2.4** The guide tool shall ask the guide server for an answer held to the answer schema — one to three ranked locations, each a path, a symbol and a line span, a kind of change and at most one edit as the arguments of an edit tool — at temperature zero, with the log-probabilities of its tokens.
- **R2.5** If the rendered state exceeds the context budget, or the guide server fails, exceeds the time limit or answers outside the schema, then the guide tool shall answer nothing and say why.
- **R2.6** If the probability the model gave its first location is below the threshold, then the guide tool shall answer nothing and say that it is not confident.
- **R2.7** The guide tool shall drop a location whose symbol the file does not declare, and answer nothing when none is left.
- **R2.8** The guide tool shall show an edit only when applying it to a copy of the file through the compiler's edit functions leaves the file checking clean and, for a failing test block, makes that block pass, and otherwise answer with the locations and the kind alone and say why the edit was withheld.
- **R2.9** The guide tool shall never write the project's files.
- **R2.10** The guide tool shall answer as JSON: the locations, the kind, the edit when shown, the confidence, and why the edit was withheld or nothing was answered.
- **R2.11** The guide command shall run the same call from the command line as `lotml guide ask`, and print the rendered messages for a state as `lotml guide render`.

## Out of scope

- Serving the model: the runtime and where it runs are plans/harness-guide.md 2.1's ADR; the tool
  speaks the OpenAI-compatible chat endpoint the candidate runtimes share.
- Calling the guide from inside `check`: the plan keeps it out of the under-100 ms check.
- A guide server off the machine: the state is the user's code.
