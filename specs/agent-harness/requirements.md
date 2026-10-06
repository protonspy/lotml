---
autonomy: auto
ci: wait
branch: feat/agent-harness
delivery: in-review
pr: 11
---

# Agent harness — requirements

## Purpose

The evaluation harness asks models for one answer at a time; agents with tools editing several
files are what it is missing (docs/wiki/pages/evaluation-harness.md). The agent harness runs a
coding agent — deepagents over a model served by OpenRouter, `z-ai/glm-5.3-flash` first — on a
benchmark of development tasks in lotml, with the compiler's MCP tools, and grades each run on
hidden tests. It measures how often the agent gets lotml right, what it costs, and whether the
`AGENTS.md` and guide `lotml init` writes (specs/agent-guide/) help.

## R1 · The benchmark

- **R1.1** The benchmark shall give each task a prompt, a starting workspace of `.lotml` files, hidden `test` blocks for each graded file, and a reference solution.
- **R1.2** The benchmark shall hold at least eight tasks that implement, fix, extend and refactor code, together exercising errors as values, `inout`, value semantics, exhaustive `match`, optionals and the absence of truthiness.
- **R1.3** The harness's tests shall show, for every task, that the reference solution passes all of its hidden tests and the starting workspace fails at least one.

## R2 · A run

- **R2.1** When a task runs, the agent harness shall copy its workspace to a fresh temporary directory and run a deepagents agent on it, over a ChatOpenRouter model, `z-ai/glm-5.3-flash` unless another is named.
- **R2.2** The agent harness shall confine the agent's file tools to the workspace and give the agent no shell.
- **R2.3** The agent harness shall give the agent the tools `lotml mcp --root <workspace>` serves, started with an environment that holds no API key.
- **R2.4** Where the context arm is `agents`, the agent harness shall run `lotml init --harness none` in the workspace and load its `AGENTS.md` as the agent's memory.
- **R2.5** Where the context arm is `reference`, the agent harness shall put the language reference in the agent's system prompt and write no `AGENTS.md`.
- **R2.6** If a run reaches its step limit or its wall-clock limit, then the agent harness shall stop the agent, grade the workspace as it stands, and record which limit stopped it.
- **R2.7** If a model call fails, then the agent harness shall record the run as an error with the reason and go on to the next run.
- **R2.8** If `OPENROUTER_API_KEY` is not set, then the agent harness shall refuse to start and say so.

## R3 · Grading

- **R3.1** When a run ends, the agent harness shall grade a copy of the workspace: `lotml check` over it, then each graded file with its hidden tests appended, through `lotml test --json`.
- **R3.2** The agent harness shall count a run as passed only when every hidden test of the task passes.

## R4 · Metrics

- **R4.1** When a run is graded, the agent harness shall append one row: task, kind, model, arm, attempt, outcome, hidden tests passed and total, whether the workspace checks, what stopped the run, model calls, tool calls by tool, tool errors, `check` calls that reported errors, input, output and reasoning tokens, cost, seconds and lines changed.
- **R4.2** The agent harness shall report, per model and arm, pass@1 with a Wilson 95% interval, pass@k by the unbiased estimator for every k up to the attempts run, the mean share of hidden tests passed, and the tokens, cost and seconds per run; and per task, the passes of each arm.
- **R4.3** When a row for a task, arm and attempt already exists, the agent harness shall skip that run, unless the row records an error, which the agent harness shall run again and count only in the errors column.
- **R4.4** The agent harness shall keep each run's messages and tool calls in a git-ignored cache, apart from the committed rows.

## Out of scope

- A comparison with Python on the same tasks; the arms compare lotml with and without the guide.
- Running untrusted benchmarks: the tasks are this repository's own, and the compiler's `test`
  tool runs the agent's code with the user's privileges, as `lotml mcp` documents.
