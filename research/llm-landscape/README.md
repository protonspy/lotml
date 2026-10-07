# llm-landscape

Three reports on what the field had measured by 2026-10-07. Each extends the wiki pages it names
and does not repeat them. Every number cites its primary source, and anything that could not be
checked there is marked: **[U]** or UNVERIFIED. Nothing was cloned or run.

- `languages.md` covers languages designed for models to write, or to program with. It gives
  each language's design, how the design was evaluated, and the results. It also collects the
  measured effect of language properties — static types, indentation, errors as values,
  verbosity and verification targets — and the intermediate-language pattern, where the model
  writes Python-shaped code and a compiler lowers it.
- `harnesses.md` covers coding-agent harnesses: SWE-agent, mini-swe-agent, OpenHands, Aider,
  Agentless, Terminal-Bench, deepagents, Claude Code, Codex CLI, Gemini CLI, OpenCode, Serena
  and the self-improving harnesses. It gives the effect of changing the harness while the model
  stays fixed, edit formats, diagnostics in the loop, tool design and test-time selection.
- `evaluation-and-adaptation.md` covers two things.
  - Evaluation: benchmarks for code generation, contamination, sandboxes, and the statistics of
    pass@k and paired comparisons.
  - Adaptation: teaching a model a language it never saw, through context, transfer, synthetic
    data, and reinforcement learning from compiler and test feedback.

The quotations went through a summarizing extraction step, so a quoted phrase is exact only
where it was read from `research/literature/cache/`. Before a number decides anything, enter it
in `research/literature/claims.json` so `verify.py` checks it against the paper.

The wiki distills these reports into:
- `docs/wiki/pages/language-design-evidence.md`
- `docs/wiki/pages/agent-harness-design.md`
- updates to `languages-for-agents`, `evaluation-harness`, `training-prior`,
  `small-coder-training` and `semantic-compiler`.

The work that follows is in `plans/agent-harness-arms.md`, `plans/evaluation-rigor.md` and
`plans/seed-corpus-rebuild.md`.
