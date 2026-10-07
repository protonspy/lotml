---
autonomy: auto
ci: wait
pr: per-group
merge: manual
status: approved
checksum: 6e7915e40d38fc85a35f04d9c9b0c38a08cd64d85bda25d678883d5c5c007602
---

# Agent harness arms

Make the harness, the provider and the compiler's tool surface into experimental variables of
the agent harness. Pin the provider and sample so that pass@k means something. Add a minimal arm,
remove the overlapping tools, and build each variable. Then measure check-on-edit, edit tools,
middleware and best-of-n one at a time.

## Why

With the model held fixed, changing the harness moves scores by 5–24 points on Terminal-Bench 2.0.
A heavier harness can sink a weak model: GPT-5-Nano fell from 44 to 14. The same weights served by
different providers ranged from 72% to 100% in tool-call schema accuracy.

lotml's harness controls none of these:
- it runs one harness;
- it records the provider each response names, but no response has named one, and no call pins
  one;
- `openrouter()` samples at temperature 0;
- the agent sees two overlapping edit vocabularies.

So a failure cannot be pinned on the language. Reporting only the errors an edit introduced is
lotml's distinction, and nobody has measured it.

Done when provider, sampling and the minimal arm are in place, the overlap is gone, each variable
of group 2 exists with tests, and each arm of group 3 has a paired result. Group 3 runs on the
scaled task set `plans/agent-data.md` produces, because eight tasks cannot show these effects. Every
run of group 3 states its model list and cost ceiling before it starts, within the OpenRouter
spending cap the owner set. See `docs/wiki/pages/agent-harness-design.md`.

## Paths

- `harness/lotml_harness/agent/`
- `harness/agent_bench/`
- `harness/results/agent/`
- `compiler/crates/lotml/src/mcp.rs`
- `compiler/crates/lotml/src/init.rs`
- `compiler/crates/lotml/src/init/`

## References

- adr:0014-deepagents-over-openrouter-for-the-agent-harness — allows a further arm
- `specs/agent-harness/` — the harness these arms extend
- `plans/agent-data.md` — the task set group 3 needs
- `docs/wiki/pages/agent-harness-design.md`, `research/llm-landscape/harnesses.md`

## Out of scope

- A bash or shell tool for the agent, or any container: either needs a new ADR, superseding adr:0014.
- Scaling the task set, which `plans/agent-data.md` owns.

## Tasks

- [ ] 1.1 (Unit) Let `openrouter()` take a provider and request only it, pin one provider per model for a comparison, and fail a run whose responses name another provider or none
- [ ] 1.2 (Unit) Make the temperature a parameter of `openrouter()`, sample above 0 whenever pass@k is reported, run each task and arm at least five times, and report pass^k beside pass@k
- [ ] 1.3 (Unit) Add a minimal arm, recorded in the agent-harness spec as a delta: a plain loop of OpenRouter tool calls over the compiler's MCP tools only. It starts the compiler only through `LotmlMcp`, never passes the OpenRouter key to the child, has no shell, and is confined to the workspace as `run.py` is
- [ ] 2.1 (Unit) Allow deepagents' edit and write tools only on paths that are not lotml sources, matched without regard to case, keeping `virtual_mode=True`, and record in each run which tool made each edit
- [ ] 2.2 (Unit) Give the MCP tools `replace`, `add`, `edit` and `rename` one or two input examples each, with purpose, limits and parameter formats inside a description's first 2,048 characters
- [ ] 2.3 (Unit) Give `digest` and `references` a limit and an offset, both capped by the server, with tests for out-of-range values, so no result nears deepagents' 20,000-token offload
- [ ] 2.4 (Unit) Keep the MCP tool list in a fixed order, and offer a lean profile of `check`, `digest`, `show`, `replace`, `edit` and `test`
- [ ] 2.5 (Unit) Have `lotml init` write, under the project directory only, a Claude Code code-intelligence plugin whose `.lsp.json` maps `lotml lsp` to `.lot` and `.lotml`, and an `lsp` entry in OpenCode's `opencode.json`. It never touches user-global configuration, never enables a plugin or adds a marketplace, keeps the command and arguments fixed, reuses `put()` and its refusal to write through links, leaves a commented or unparseable configuration alone and reports it, and is idempotent, with a test for each
- [ ] 2.6 (Unit) Add check-on-edit report modes to the MCP server, selected per run: no report, every current error, or the errors an edit introduced (today's)
- [ ] 2.7 (Unit) Add three harness middlewares, each behind its own flag: a check and the tests must pass before the agent may finish; a nudge after the same diagnostic code or edits of the same symbol repeat N times; the `digest` in the first message
- [ ] 2.8 (Unit) Add best-of-n: k attempts per task, the selected one being the first that passes `lotml check` and the visible `test` blocks
- [ ] 3.1 (Unit) Measure the harness as arms on the same tasks, model and provider: deepagents against the minimal arm, paired per task, with pass@1, pass^k and cost
  _Depends 1.1, 1.2, 1.3_
- [ ] 3.2 (Unit) Measure the three check-on-edit report modes as arms, recording failed edits, recovery after a failed edit and rounds to green
  _Depends 2.6, 3.1_
- [ ] 3.3 (Unit) Measure the edit tools exposed as arms (symbol edits only, text edits only, both), recording failed applications and well-formed edits per model
  _Depends 2.1, 3.1_
- [ ] 3.4 (Unit) Measure each of the three middlewares as its own arm against none
  _Depends 2.7, 3.1_
- [ ] 3.5 (Unit) Measure best-of-n: how often some attempt passed the hidden tests against how often the selected one did, with and without the selector
  _Depends 2.8, 3.1_

## Done when

- Every graded run in `harness/results/agent/` names its provider, temperature, sample count and cost.
- `harness/results/` reports each arm of group 3 with paired results.
- `uv --directory harness run pytest`, `cargo test --manifest-path compiler/Cargo.toml`, ruff and clippy pass.
- `scc validate` exits 0.
