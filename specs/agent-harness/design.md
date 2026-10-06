# Agent harness — design

## What changes

Serves R1.1–R1.3, R2.1–R2.8, R3.1–R3.2, R4.1–R4.4.

A package `harness/lotml_harness/agent/` and a benchmark under `harness/agent_bench/`.
Agent framework: adr:0014-deepagents-over-openrouter-for-the-agent-harness.

```
harness/agent_bench/<task>/
  task.toml        id, kind, graded files, optional step and time limits
  prompt.md        what the agent is asked, as a user would ask it
  workspace/       the starting project
  hidden/<file>    test blocks appended to the graded file of that name
  solution/        the files the reference solution changes, laid over workspace/
harness/lotml_harness/agent/
  bench.py         load tasks; lay a solution over a workspace
  grade.py         check, then hidden tests per graded file, in a scratch copy
  mcp.py           stdio JSON-RPC client of `lotml mcp`, its tools as LangChain tools
  run.py           one run: workspace, agent, limits, trace, metrics row
  report.py        pass@k, Wilson interval, the Markdown report
  __main__.py      python -m lotml_harness.agent --model … --arm … --attempts …
```

**Hidden tests are appended, not imported.** lotml has no imports between a project's own modules
(`from stock import …` is E0216: only `math`, bound Python modules and C libraries), so a hidden
test cannot sit in a file of its own. Grading appends `hidden/<file>` to the agent's `<file>` in a
scratch copy, as phase 1 judges an answer, and runs `lotml test --json` on it. Hidden test names
start with `hidden:` so they never collide with the agent's own.

**The agent.** `create_deep_agent(model, tools, system_prompt, backend, memory)`:

- `model` — `ChatOpenRouter(model=…, temperature=0)`, the key from `OPENROUTER_API_KEY` (R2.8).
- `backend` — `FilesystemBackend(root_dir=workspace, virtual_mode=True)`. It implements no
  `execute`, so the agent's `execute` tool answers with an error instead of a shell (R2.2);
  `LocalShellBackend` would run any command on the host.
- `tools` — the compiler's, from `mcp.py` (R2.3). A dozen lines of JSON-RPC over the server's
  newline framing, synchronous, in the way the compiler speaks it with no protocol library; one
  server per run, started with `execute.child_environment()` so the agent's code, run by the
  `test` tool, never sees the key.
- `memory=["/AGENTS.md"]` in the `agents` arm (R2.4), after `lotml init --harness none`; the
  `reference` arm puts `variants.reference_text("b")` in the system prompt instead (R2.5).
- Limits — LangGraph's `recursion_limit` for steps, and the run on a worker thread joined with the
  wall-clock limit (R2.6). Defaults: 60 steps, 600 s.

**Metrics** are read from the messages the agent leaves (R4.1): each `AIMessage`'s
`usage_metadata` (input, output, reasoning tokens) and its `tool_calls`; each `ToolMessage`'s
status for tool errors; `check` results holding a diagnostic for check loops. Cost is the tokens
times the model's prices, fetched once from OpenRouter's public `/models` listing, since a
response's own cost field is not carried through every LangChain version. Lines changed is a
`difflib` count against the starting workspace.

**Files.** Rows go to `harness/results/agent/<model>__<arm>.jsonl` and the report to
`harness/results/agent.md`, committed like every experiment's answers; traces to
`harness/cache/agent/`, already git-ignored (R4.4).

**pass@k** is Chen et al.'s unbiased estimator, `1 - C(n-c, k) / C(n, k)` per task, averaged;
the Wilson interval is over pass@1's runs.

## Data

One row per run:

```json
{"task": "stock-take", "kind": "feature", "model": "z-ai/glm-5.3-flash", "arm": "agents",
 "attempt": 0, "outcome": "pass", "hidden": [7, 7], "checks": true, "stopped": "done",
 "model_calls": 14, "tools": {"check": 4, "edit_file": 3}, "tool_errors": 1, "check_errors": 2,
 "tokens": {"input": 51234, "output": 2210, "reasoning": 830}, "cost": 0.0089,
 "seconds": 71.4, "lines_changed": 23, "error": null}
```

`outcome` is `pass`, `fail` (checks, some hidden test fails), `no check` (the workspace has
errors) or `error` (the run did not finish: model failure). `stopped` is `done`, `steps` or
`time`.

## Alternatives considered

- **langchain-mcp-adapters** for the compiler's tools: one more dependency, asynchronous, and an
  MCP client negotiating protocol versions with a server that speaks two eras of it; the server's
  newline JSON-RPC takes less code to speak than to configure.
- **ChatOpenAI pointed at OpenRouter's URL**: works, but the user chose the OpenRouter
  integration, which takes OpenRouter's provider routing natively.
- **Python on the same tasks** as a control: worth having, and a second benchmark to write; the
  first question is whether the agent writes lotml at all, and whether the guide helps.

## Risks

- The model may route through different providers between runs; the provider is recorded per row
  when the response names it.
- Eight tasks are far below the 168 paired tasks a 10-point difference needs
  (docs/wiki/pages/evaluation-harness.md): this benchmark exercises the harness and shows where
  the agent fails, it does not settle a gate.
