---
autonomy: auto
ci: wait
branch: fix/guide-request
delivery: merged
pr: 24
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
checks, says nothing when it is unsure, and never edits. The model's answer is untrusted input: it
is validated before anything acts on it, and the code it proposes runs only where nothing it can
reach is dangerous.

## R1 · Configuration

- **R1.1** Where a harness guide configuration is found, in the file `LOTML_HARNESS_GUIDE` names or else in `harness-guide.toml` in the user's lotml configuration directory, the MCP server shall list a `guide` tool and name it in its instructions.
- **R1.2** The MCP server shall read from the configuration the guide server's URL, the model, the confidence threshold, the answer budget, whether candidate edits may run, the renderer version and the records the guide was trained from, and refuse a key it does not know.
- **R1.3** If no configuration is found, then the MCP server shall list no `guide` tool and answer every other tool as before.
- **R1.4** If the configuration cannot be read, is named by a relative path, names a URL other than `http://` to `127.0.0.1`, `[::1]` or `localhost` with a decimal port and nothing after it, or names a renderer version other than the compiler's, then the MCP server shall list no `guide` tool and say why on standard error.

## R2 · The call

- **R2.1** When the agent calls `guide`, with the optional `paths` and `task` its input schema names, the guide tool shall check the files named, or all of the project's that are not symbolic links, and take as the state the first file with errors and its diagnostics, or else the first failing test block with the values each side of its comparison had.
- **R2.2** If the files check and every test block passes, then the guide tool shall answer that there is nothing to guide without asking the guide server.
- **R2.3** The guide tool shall render the guide's system message and the state — the task when one is given, the file with its lines numbered, the diagnostics or the failing block — with one renderer, which the guide command also applies to a state given as JSON.
- **R2.4** The guide tool shall ask the guide server for an answer held to the answer schema — one to three ranked locations, each a path, a symbol and a line span, a kind of change and at most one edit as the arguments of an edit tool — at temperature zero, within the answer budget, with the log-probabilities of its tokens.
- **R2.5** The guide tool shall finish every call within one deadline under 90 seconds, covering the state's tests, the request and the candidate's tests, and answer nothing when the deadline passes.
- **R2.6** If the guide server refuses the request as longer than its context, fails, answers with a status other than 200 or with more than the size limit, or gives an answer outside the schema, then the guide tool shall answer nothing and say why.
- **R2.7** If the probability the model gave its first location is below the threshold, then the guide tool shall answer nothing and say that it is not confident.
- **R2.8** The guide tool shall keep a location only when its path is exactly one of the project's files, its symbol, when it names one, is declared in that file, and its lines lie within the file, keep an edit only when it targets a kept location's file, and answer nothing when no location is left.
- **R2.9** The guide tool shall show an edit only when, applied to a copy of the file through the compiler's edit functions, it leaves the text of every test block unchanged and the file checking clean, and, for a failing test block, makes that block pass in a private copy of the project.
- **R2.10** Where candidate edits may not run, or the project binds a Python module or a C library through an interface, the guide tool shall run no candidate and withhold an edit for a failing test block.
- **R2.11** The guide tool shall never write the project's files.
- **R2.12** The guide tool shall answer as JSON — the locations, the kind, the edit when shown, the confidence, and a reason from a fixed list when an edit is withheld or nothing is answered — built from the validated answer, never from other text of the guide server's response.
- **R2.13** The guide command shall run the same call as `lotml guide ask`, taking files, `--task` and `--root`, and print the rendered messages for a state given as JSON as `lotml guide render`.

## Out of scope

- Serving the model: the runtime and where it runs are plans/harness-guide.md 2.1's ADR; the tool
  speaks the OpenAI-compatible chat endpoint the candidate runtimes share.
- Calling the guide from inside `check`: the plan keeps it out of the under-100 ms check.
- A guide server off the machine: the state is the user's code.
