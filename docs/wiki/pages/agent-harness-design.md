# Agent harness design

How the coding harnesses of 2026 are built, what changing a harness does to a fixed model's score,
and which design choices have a measured effect. lotml's agent harness runs deepagents over
OpenRouter (adr:0014-deepagents-over-openrouter-for-the-agent-harness), and its compiler is itself
a tool for agents ([[semantic-compiler]]). This page says what the field's evidence asks of both.
Details and sources are in `research/llm-landscape/harnesses.md`. Its numbers were read on their
primary sources, but none is yet in `research/literature/claims.json`, so none is machine-checked.

## The harness moves the score as much as the model

With the model held fixed, published pairs of harnesses differ by:
- 5–24 points on Terminal-Bench 2.0;
- 5–8 points on SWE-bench Verified;
- up to 36 points on CORE-Bench Hard.

| model | benchmark | one harness, then another | difference |
| --- | --- | --- | ---: |
| Claude Haiku 4.5 | Terminal-Bench 2.0 | OpenHands 13.3, mini-swe-agent 29.8 | +16.5 |
| GPT-5 | Terminal-Bench 2.0 | Terminus 2 35.2, Codex CLI 49.6 | +14.4 |
| GPT-5.2-Codex | Terminal-Bench 2.0 | deepagents 52.8, deepagents after harness work 66.5 | +13.7 |
| Claude Opus 4.5 | Terminal-Bench 2.0 | Claude Code 52.1, Terminus 2 57.8 | +5.7 |
| GPT-5-Nano | SWE-bench Verified, 50 tasks | mini-swe-agent 44.0, Live-SWE-agent 14.0 | −30.0 |

Sources: Terminal-Bench 2.0's Table 2 ([arXiv 2601.11868](https://arxiv.org/abs/2601.11868)),
LangChain's [harness engineering post](https://www.langchain.com/blog/improving-deep-agents-with-harness-engineering),
and Live-SWE-agent ([arXiv 2511.13646](https://arxiv.org/abs/2511.13646)).

Two lessons follow for lotml:

- **Which harness wins depends on the model.** Claude Code is not the best harness for Claude
  models on Terminal-Bench. A harness that helps a strong model can sink a weak one: GPT-5-Nano
  could not use the tools it was asked to build, and looped.
- **deepagents' own gain came from middleware, prompt and tools alone:** a forced check before
  the agent may finish, loop detection, and a snapshot of the environment at the start. Nobody
  ablated those changes one at a time.

lotml's first agent model, `z-ai/glm-5.3-flash`, is a cheap one. Without a minimal arm — a plain
loop of tool calls over the compiler's MCP tools — a failure cannot be pinned on the language
rather than on deepagents. adr:0014 already allows a further arm.

## The provider is a variable too

The same open weights, served by different OpenRouter providers, ranged from 71.96% to 100% in
the accuracy of their tool-call schemas
([MoonshotAI/K2-Vendor-Verifier](https://github.com/MoonshotAI/K2-Vendor-Verifier)). Epoch ranks
the choice of provider next to the choice of harness. lotml's harness records the provider each
response names, but its graded runs so far name none, and no call pins one. A comparison pins one
provider per model, records it, and fails a run served by another.

## Sampling

- **Temperature 0 measures the provider, not the model.** Repeated attempts at temperature 0
  measure the provider's nondeterminism, so pass@k computed from them misleads.
- **Sample above 0, run each task five times or more** (Terminal-Bench's practice), and report
  pass^k — all k attempts succeed — beside pass@k.
- **Sample size limits what can be seen.** At about 200 paired tasks, one sample per task can
  show a difference of about 13 points, and ten samples about 7.5
  ([[evaluation-harness]]). The harness effects above are visible at that scale. A 3-point
  effect like SWE-agent's lint gate is not.

## Edits: precise, addressed, and still a choice per model

- **Self-improving harnesses converged on precise edits.** DGM, SICA, Live-SWE-agent and AHE
  each started from bash and each independently arrived at string- or symbol-addressed edits,
  verification of each edit, and a way to locate symbols. OpenAI and aider removed line
  numbers.
- **Weak open models do better rewriting whole units.** Qwen2.5-Coder-32B solved 16.4% of aider's
  polyglot benchmark rewriting whole files and 8.0% with search and replace. At 0.5B the gap was
  90 against 50 ([arXiv 2609.05779](https://arxiv.org/abs/2609.05779)).
- **Tolerant matching is common practice.**
  - aider accepts a uniform indentation offset.
  - Gemini CLI re-applies the target's indentation.
  - Codex trims whitespace and normalizes punctuation.

  lotml's `edit` does the same ([[editing-robustness]]).

lotml keeps both `replace`, which rewrites a whole unit, and `edit`, which works on line text.
Which one each model uses is worth recording rather than forcing.

## Diagnostics inside the loop

- **Reporting diagnostics after every edit is now standard.**
  - Claude Code does it once a code-intelligence plugin registers a language server; without
    one, its LSP tool is inactive.
  - OpenCode appends errors only, at most 20 per file, for the edited file.
  - Cursor exposes its lints as a tool.
- **Nobody has evaluated reporting only the diagnostics an edit introduced.** That is lotml's
  check-on-edit ([[semantic-compiler]]). SWE-agent's lint gate (+3 points, one run) is the only
  measured gate. Comparing no report, all current errors, and introduced errors only would be
  evidence that does not exist yet.

## Tools

- **Failures are in choosing the tool and filling its parameters, not in schema syntax.** Tool
  use is 56.7% of failures in MCP-Atlas ([arXiv 2602.00933](https://arxiv.org/abs/2602.00933)),
  and wrong-tool selection between near-synonyms is the failure Anthropic reports most.
- **lotml's agent sees two edit vocabularies.** deepagents' unprefixed `edit_file`, `read_file`
  and `grep` sit beside the compiler's `edit`, `show` and `references`. Prefixing the compiler's
  tools, or denying the built-in edit tools on `.lot` files, removes the overlap.
- **Input examples help.** One or two input examples per tool took complex parameters from 72% to
  90% in Anthropic's internal test. Fully augmented descriptions cost 67% more steps
  ([arXiv 2602.14878](https://arxiv.org/abs/2602.14878)).
- **Long tool results get cut or hidden.**
  - Accuracy falls as JSON tool responses grow.
  - deepagents offloads any result over 20,000 tokens to a file, behind a 10-line preview.
  - Claude Code warns past 10,000 tokens.

  `digest` and `references` need a limit or a concise mode.
- **A fixed tool order keeps the prompt cache warm.** Codex reported cache misses from tools
  listed in an inconsistent order.

## Selecting among attempts

The limit of best-of-n is selection, not generation. Four systems show a gap of 10–14 points
between how often some attempt was right and how often the selected one was. Cheap filters
recover part of it: visible tests, regression tests, and rejecting patches that touch only
tests. `lotml check` plus a program's visible `test` blocks is such a filter, at no model cost.

## What lotml measures, in order of expected effect

1. **Harness arm:** deepagents, a minimal MCP loop, and later Claude Code or Codex headless, on
   the same tasks, model and provider.
2. **Provider:** one model through two pinned providers, counting tool-call parse and schema
   failures.
3. **Check-on-edit report:** none, all current errors, or introduced errors only.
4. **Edit tools exposed:** symbol edits only, text edits only, or both, with deepagents' own edit
   tools kept off lotml sources.
5. **Middleware, one piece at a time:** the check before finishing, loop detection, and the
   `digest` at the start.
6. **Best-of-n with the check-and-test filter:** coverage against what was selected.

The work is in `plans/agent-harness-arms.md`.
