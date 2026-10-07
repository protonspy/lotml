# Agent harnesses for coding: design, evaluation methodology and measured effects

Study for LotML, written 2026-10-07. Scope: how the leading coding harnesses are built, how their design
choices were evaluated, what the numbers say, and what that means for LotML's agent harness
(deepagents over OpenRouter, adr:0014-deepagents-over-openrouter-for-the-agent-harness) and for the
compiler's agent-facing features (JSON diagnostics, symbol-addressed edits, LSP, MCP).

Method. Every number was fetched from the primary source named next to it (paper full text, official
repository, official blog or documentation, official leaderboard). The fetches went through a summarising
reader, so the numbers this report leans on hardest were fetched a second time or read from raw files
(Terminal-Bench Table 2, the LangChain deepagents post, Live-SWE-agent Tables 4–5, aider's polyglot YAML,
the AHE, SWE-Edit, Complexity Trap and Harness Engineering abstracts). Anything not confirmed on a primary
page is marked UNVERIFIED. Repository stars, licences and last-push dates come from the GitHub API on
2026-10-07. No code was cloned or run. "pp" means percentage points.

## Summary

1. The harness moves the score as much as a model generation does: with the model fixed, published pairs differ by 5–24 pp on Terminal-Bench 2.0, 5–8 pp on SWE-bench Verified and up to 36 pp on CORE-Bench Hard.
2. The direction is not universal. Claude Code is not the best harness for Claude models on Terminal-Bench (Opus 4.5: Claude Code 52.1, Terminus 2 57.8), and a richer harness can wreck a weak model (GPT-5-Nano: 44.0 under mini-swe-agent, 14.0 under Live-SWE-agent).
3. deepagents itself, LotML's harness, gained 13.7 pp on Terminal-Bench 2.0 (52.8 to 66.5, GPT-5.2-Codex) from prompt, tool and middleware changes alone. No per-change ablation was published.
4. Interface ablations with the model fixed are single-digit pp and usually single runs. SWE-agent's: no edit command −7.7, unbounded iterative search −6.0, full-file viewer −5.3, lint gate −3.0.
5. Every self-improving harness (DGM, SICA, Live-SWE-agent, AHE) independently converged on precise string- or symbol-addressed edits, edit verification and symbol location. Gains came from tools, middleware and memory, not the system prompt.
6. Edit format matters most for weak models. Qwen2.5-Coder-32B solved 16.4% of aider's polyglot benchmark rewriting whole files and 8.0% with search/replace (99.6% against 71.6% well-formed). Frontier models are near 100% well-formed in any format.
7. Diagnostics after every edit are now standard: Claude Code (via LSP plugins), OpenCode, Cursor's read_lints. Nobody publishes an evaluation of reporting only the diagnostics an edit introduced. LotML's diff against the pre-edit state is untested ground.
8. Best-of-n is limited by selection, not generation. Four systems show a 10–14 pp gap between oracle coverage and the selected patch. Cheap filters (visible tests, regression tests) recover 1–5 pp.
9. Tool interfaces fail on selection and parameters, not on schema syntax. Fewer, distinct, compactly described tools with examples help, and providers serving the same open weights differ in tool-call schema accuracy (Kimi K2: 71.96% to 100%).
10. For LotML: treat the harness and the OpenRouter provider as experimental variables, add a minimal arm, pin and record providers, and measure check-on-edit and symbol edits as ablations. None of the evidence argues against the compiler-as-tool design.

## What is new versus the wiki pages

The wiki ([[semantic-compiler]], [[editing-robustness]], [[evaluation-harness]], [[rl-environment]],
[[repair-training]]) already holds: SWE-agent's lint ablation and its noise band; aider's 9x flexible-patching claim; Agentless's skeleton result and its 32.00% at $0.70; CODESTRUCT, Diff-XYZ, *To Diff or Not to Diff*, RustAssistant; the LSP-for-agents study arXiv 2608.13568, generative compilation, LocAgent and SweRank localization.

This report adds:
- **Harness effect sizes with the model held fixed:** Terminal-Bench 2.0 Table 2, HAL, Epoch, Meta-Harness, Confucius Code Agent, the swebench.com bash-only leaderboard against vendor scaffolds, METR.
- **The whole SWE-agent ACI ablation table**, not just the lint row, read against its six-run noise.
- **deepagents' own harness-engineering result**, and its context defaults (offload at 20k tokens, summarise at 85% of the window).
- **Self-improving and auto-designed harnesses** (Live-SWE-agent, DGM, SICA, HGM, Meta-Harness, AHE): what they discovered.
- **Aider's per-model format numbers** (polyglot well-formed rates, whole against diff, architect/editor), and the editing tools of Codex CLI and Gemini CLI.
- **Context management, measured:** observation masking matches LLM summarisation at half the cost.
- **Test-time scaling and verifiers**, with the coverage-to-selection gap.
- **MCP and tool-design evidence:** tool count, description quality, output size, the spec's structured content.
- **Provider variance on OpenRouter** for the same open weights.
- **One correction.** [[semantic-compiler]] says Claude Code "returns LSP diagnostics after every edit". The official docs say this happens only once a code-intelligence plugin is installed; without one the LSP tool is inactive (https://code.claude.com/docs/en/tools-reference#lsp-tool-behavior).

## Per-harness detail

### SWE-agent: the agent-computer interface (arXiv 2405.15793, NeurIPS 2024)

**Design.** A shell plus custom commands: search (`find_file`, `search_dir`, `search_file`), with results summarised; a 100-line file viewer; a line-range `edit` that runs a linter and reverts bad edits; a history that keeps the last 5 observations in full; one demonstration trajectory.

Its principles, quoted: actions "simple and easy to understand", "compact and efficient"; feedback
"informative but concise"; "guardrails mitigate error propagation".

**Evaluation.**
- SWE-bench full (2,294) and Lite (300); GPT-4 Turbo and Claude 3 Opus; configuration chosen on 37 development instances.
- Ablations on Lite, GPT-4 Turbo, one run each.
- Six default runs gave pass@1 17.94%, pass@3 27.35%, pass@6 32.67% (Table 10).

**Results.**
- GPT-4 Turbo: 12.47% full, 18.00% Lite. Claude 3 Opus: 10.46% / 13.00%.
- A shell-only agent: 11.00% Lite, a "64% relative increase" for the ACI.

Ablations (Table 3), default 18.0:

| Change | Lite resolved (%) | Δ (pp) |
| --- | ---: | ---: |
| No edit command | 10.3 | −7.7 |
| Iterative search | 12.0 | −6.0 |
| Full-file viewer | 12.7 | −5.3 |
| 30-line window | 14.3 | −3.7 |
| Full history | 15.0 | −3.0 |
| No linting | 15.0 | −3.0 |
| No search | 15.7 | −2.3 |
| No demonstration | 16.3 | −1.7 |

Against the 17.33–18.67 spread of six default runs, only the first three clearly exceed noise. Unbounded
iterative search did worse than no search at all (https://arxiv.org/html/2405.15793).

**Status.** Later 1.x results: 48.0% Lite with Claude 3.7 Sonnet; 66.6% Verified with Claude 4 Sonnet on the
official leaderboard JSON, though the submission's README says 69%. The docs now call SWE-agent
"maintenance-only" and recommend mini-swe-agent (https://swe-agent.com/latest/).

**Repo.** https://github.com/SWE-agent/SWE-agent · MIT · 20,497 stars · pushed 2026-10-06.

### mini-swe-agent and the bash-only leaderboard

**Design.** About 100 lines of Python. The only action is bash, run with `subprocess.run` (no persistent
shell), over a linear history. v1 parsed actions from text; v2 made native tool calling the default
(https://mini-swe-agent.com/latest/advanced/v2_migration/).

**Evaluation.** swebench.com runs every model "using mini-SWE-agent in a minimal bash environment. No tools,
no special scaffold structure", with one untuned configuration. It warns that 1.x and 2.x results are not
comparable.

**Results.** Verified, 500 tasks, one attempt
(https://raw.githubusercontent.com/SWE-bench/swe-bench.github.io/master/data/leaderboards.json): Claude 4.5 Opus (high, v2) 76.8; Gemini 3 Flash (high) 75.8; Claude Sonnet 4.5 (v1.13.3) 70.6; GPT-5.1 (medium) 66.0; Claude Sonnet 4 (v1.0.0) 64.93; Claude 4 Sonnet in the same JSON: SWE-agent 66.6, OpenHands 70.4; Sonnet 4.5: Anthropic reports 77.2 with its own bash plus string-replace scaffold (10 trials, 200K thinking budget; https://www.anthropic.com/news/claude-sonnet-4-5).

The "roulette" post, on 50 instances (https://www.swebench.com/post-250820-mini-roulette.html): GPT-5 alone
32, Sonnet 4 alone 33, switching randomly between them per step 39. Gains "become marginal at around a 50
step limit".

**Repo.** https://github.com/SWE-agent/mini-swe-agent · MIT · 8,271 stars · pushed 2026-10-06.

### OpenHands and CodeAct

**CodeAct** (arXiv 2402.01030, ICML 2024) uses Python code as the action space instead of JSON or text tool
calls. Evaluation: 17 LLMs, on API-Bank (atomic calls) and M3ToolEval (82 multi-tool tasks).
- GPT-4-1106 on M3ToolEval: 74.4% with code, 52.4% JSON, 53.7% text; 5.5 turns against 7.6.
- Code was best for 12 of 17 models.
- On atomic API-Bank calls JSON can win: GPT-4-1106 JSON 82.7 against code 76.7.
- Code pays where tools compose, not for single calls (https://arxiv.org/html/2402.01030).

Repo: https://github.com/xingyaoww/code-act · MIT · 1,709 stars · pushed 2024-05-23.

**OpenHands** (arXiv 2407.16741, ICLR 2025).
- **Design.** An event stream holds the agent state; a Docker sandbox holds bash, IPython and a browser; CodeActAgent is the default.
- **Results.** SWE-bench Lite: CodeActAgent v1.8 with Claude 3.5 Sonnet 26.0% at $1.10. The paper has no ablations.
- **SDK paper** (arXiv 2511.03690 v2): event-sourced state, MCP tools as first-class tools, a condenser. System-attributable production failures fell from 78.0 to 30.0 per 1k conversations.
- **V0 against V1 on Verified:** Sonnet 4 68.0/68.0. The Sonnet 4.5 gain (64.6 to 72.8) is credited to extended-thinking support. No tool or condenser ablations (https://arxiv.org/html/2511.03690).

Blogs:
- **Condenser:** "up to 2x" lower per-turn cost, 54% against 53% solve rate on an unnamed subset (https://openhands.dev/blog/openhands-context-condensensation-for-more-efficient-ai-agents).
- **Critic:** a Qwen2.5-Coder-32B critic over Claude 3.7 Sonnet at temperature 1.0 took a single rollout from 60.6% to 66.4% best-of-5 on Verified (https://openhands.dev/blog/sota-on-swe-bench-verified-with-inference-time-scaling-and-critic-model).
- **Leakage:** on Commit0, agents mined git history; shallow clones dropped MiniMax-M2.5 from 50% to 18.8% (https://openhands.dev/blog/analyzing-and-improving-openhands-index).

Repos: https://github.com/OpenHands/OpenHands · MIT · about 90,200 stars (value possibly rounded by the reader) · pushed 2026-10-07; https://github.com/OpenHands/software-agent-sdk · MIT · 1,208 stars.

### Aider: edit formats, repo map, polyglot benchmark

**Design.**
- **Formats:** `whole`, `diff` (SEARCH/REPLACE), `diff-fenced` (for Gemini), `udiff` (for GPT-4 Turbo's laziness), `editor-diff`/`editor-whole` (architect mode). An unlisted model defaults to `whole`.
- **Mismatch fallbacks:** exact match, then a uniform leading-whitespace offset, then `...` elisions. Fuzzy edit distance exists but is disabled. A malformed edit is fed back, at most 3 reflections (https://raw.githubusercontent.com/Aider-AI/aider/main/aider/coders/editblock_coder.py).
- **Repo map:** tree-sitter definitions and references, ranked with PageRank, 1k-token default budget (https://raw.githubusercontent.com/Aider-AI/aider/main/aider/repomap.py).
- **Lint:** tree-sitter syntax errors plus fatal flake8 codes, shown inside the enclosing function and fed back automatically (https://aider.chat/2024/05/22/linting.html).
- Neither the repo map nor the lint loop has a published measured effect.

**Evaluation.**
- First 133 Exercism Python exercises, then the polyglot benchmark (https://aider.chat/2024/12/21/polyglot.html).
- Polyglot keeps 225 of 697 problems that 3 or fewer of 7 models solved: C++ 26, Go 39, Java 47, JavaScript 49, Python 34, Rust 30.
- Two tries, the second with test errors. "Well-formed" is the share of exercises with no malformed reply.

**Results.**
- **Format, same model:**
  - gemini-exp-1206: whole 80.5% (100% well-formed), diff 69.2% (84.2%).
  - o1-preview: whole 79.7% (100%), diff 75.2% (84.2%).
  - Qwen2.5-Coder-32B on polyglot: whole 16.4% (99.6%), diff 8.0% (71.6%).
  - Sources: https://aider.chat/docs/leaderboards/edit.html, https://aider.chat/2024/09/12/o1.html, and the raw polyglot YAML, re-fetched.
- **Laziness benchmark** (89 refactorings, https://aider.chat/docs/unified-diffs.html): GPT-4 Turbo: SEARCH/REPLACE 20%, udiff 61%; lazy comments on 12 tasks against 4; Dropping "high level diff" prompting raised editing errors 30–50%; Line numbers were removed: "GPT is terrible at working with source code line numbers".
- **Long outputs collapse compliance** (refactor leaderboard, pass / well-formed): gpt-4o 62.9 / 53.9; gemini-1.5-pro diff-fenced 49.4 / 7.9 (https://aider.chat/docs/leaderboards/refactor.html).
- **Architect/editor:** a reasoner describes the change, an editor model writes it (https://aider.chat/2024/09/26/architect.html).

  | Setup | Paired | Alone |
  | --- | ---: | ---: |
  | o1-preview + DeepSeek | 85.0% | 79.7% |
  | Sonnet + Sonnet | 80.5% | 77.4% |
  | GPT-4o + GPT-4o | 75.2% | 71.4% |
  | GPT-4o-mini + GPT-4o-mini | 60.2% | 55.6% |

- **Polyglot leaders:** gpt-5 (high) 88.0% at 91.6% well-formed; gemini-2.5-pro 83.1% at 99.6% (https://aider.chat/docs/leaderboards/).
- **SWE-bench Lite, 26.3%:** up to six attempts alternating GPT-4o and Opus. An attempt counted as "plausible" only with no edit, lint or test errors. The first GPT-4o attempt alone scored 20.3% (https://aider.chat/2024/05/22/swe-bench-lite.html).

**Repo.** https://github.com/Aider-AI/aider · Apache-2.0 · 49,411 stars · last push 2026-05-22 (quiet for 4.5 months; leaderboard last updated 2025-11-20).

### Agentless, AutoCodeRover, Moatless and SWE-Search

**Agentless** (arXiv 2407.01489) is a fixed pipeline: hierarchical localization: files, then skeleton classes and functions, then lines; 40 sampled SEARCH/REPLACE patches; selection by regression tests, generated reproduction tests and majority vote.

Ablations, Lite, GPT-4o (https://arxiv.org/html/2407.01489):
- **Localization:** hierarchical edit localization 50.67% at $0.06, against 47.00% at $0.18 going straight from files.
- **Selection:** majority vote alone 25.67%; plus regression tests 27.00%; plus reproduction tests 32.00%. A perfect selector over all samples would reach 42.0%.

OpenAI used Agentless as "best-performing open-source scaffold" for o1: 40.9% Verified
(https://arxiv.org/html/2412.16720). The README later reports 50.8% Verified with Claude 3.5 Sonnet.

Repo: https://github.com/OpenAutoCoder/Agentless · MIT · 2,122 stars · inactive since 2024-12-22.

**AutoCodeRover** (arXiv 2404.05427, ISSTA 2024).
- **Design:** AST-indexed search APIs (`search_class`, `search_method_in_class`, …), optional spectrum-based fault localization, up to 3 retries on invalid patches.
- **Results**, GPT-4, three runs: Lite pass@1 19%, pass@3 26%, at $0.43 and 37k tokens per task (SWE-agent: $2.51).
- **Ablation:** fault localization added 3 pp (57 to 66 resolved). AST search alone was never ablated (https://arxiv.org/html/2404.05427).

Repo: https://github.com/AutoCodeRoverSG/auto-code-rover · Sonar Source-Available Licence (not open source; Sonar acquired it 2025-02-19) · about 3,100 stars · pushed 2025-04-24.

**Moatless Tools:** "build good tools to insert the right context". Lite: Claude 3.5 Sonnet 26.67% at $0.17
(Agentless Table 1). The README claims 70.8% with Claude 4 Sonnet at $0.63, without naming the split.

**SWE-Search** (arXiv 2410.20285): MCTS with a value agent and a discriminator, over Moatless.
- GPT-4o 25.7 to 31.0; Qwen2.5-72B 18.0 to 24.7; mean +23% relative across five models.
- GPT-4o cost rose from $40.86 to $576.

Repos: https://github.com/aorwall/moatless-tools · MIT · 643 stars; https://github.com/aorwall/moatless-tree-search · Apache-2.0 · 143 stars.

### Terminal-Bench 2.0, Terminus 2 and Harbor

**Benchmark** (arXiv 2601.11868, ICLR 2026).
- 89 tasks, kept from 229 submitted. Each task: a Docker image, an instruction, tests of the final container state, a reference solution, a time limit.
- About three reviewer-hours per task, an adversarial exploit agent, and two more auditors.
- Every model and agent pair ran each task at least five times: 32,155 trials in total.

**Terminus 2**, the paper's "neutral testbed":
- One tool, a tmux terminal. The model answers in JSON or XML with analysis, plan, keystrokes and `task_complete`.
- Summarisation near the context limit; `task_complete` needs a second confirmation (https://raw.githubusercontent.com/harbor-framework/harbor/main/src/harbor/agents/terminus_2/terminus_2.py).

**Results.** Table 2, re-fetched, intervals about ±2.5–3.1 (https://arxiv.org/html/2601.11868v1):

| Model | Results by harness |
| --- | --- |
| GPT-5 | Codex CLI 49.6, OpenHands 41.5, Terminus 2 35.2, mini 33.9 |
| GPT-5.2 | Codex CLI 62.9, Terminus 2 54.0 |
| Claude Opus 4.5 | Terminus 2 57.8, Claude Code 52.1, OpenHands 51.9 |
| Claude Haiku 4.5 | mini 29.8, Terminus 2 28.3, Claude Code 27.5, OpenHands 13.3 |
| Gemini 2.5 Pro | Terminus 2 32.6, mini 26.1, Gemini CLI 19.6, OpenHands 15.7 |
| Sonnet 4.5 | four harnesses within 40.1–42.8 |

Opus 4.5 consumed 256.9M input tokens under Claude Code against 3.9M under Terminus 2. The authors conclude
"model selection is usually more important than agent scaffold"; their own examples are a 51.4 pp model
swing against a 16.9 pp scaffold swing.

**Repos.**
- https://github.com/harbor-framework/harbor · Apache-2.0 · 5,891 stars · pushed 2026-10-07.
- https://github.com/harbor-framework/terminal-bench-2 · Apache-2.0 · 423 stars.
- The board is now at version 4.0 (https://www.tbench.ai/benchmarks) and shows vendor CLIs only.

### deepagents (LotML's harness)

**Design.**
- Built-ins: `ls`, `read_file`, `write_file`, `edit_file`, `glob`, `grep` (`execute` with a sandbox backend), a `task` tool for subagents, planning.
- Context engineering (https://docs.langchain.com/oss/python/deepagents/context-engineering): tool inputs or results over 20,000 tokens are offloaded to the filesystem, replaced by a path and a 10-line preview; older tool calls are truncated past 85% of the window; summarisation triggers at 85% of `max_input_tokens` and keeps 10% as recent context.

**Evaluation and results.** "Improving Deep Agents with harness engineering"
(https://www.langchain.com/blog/improving-deep-agents-with-harness-engineering):
- GPT-5.2-Codex fixed, Terminal-Bench 2.0 (89 tasks): 52.8 to 66.5 (+13.7 pp), from outside the top 30 to the top 5.
- The changes: build-and-verify guidance with a `PreCompletionChecklistMiddleware`; a `LocalContextMiddleware` that maps the environment at the start; a `LoopDetectionMiddleware` that counts edits per file; time-budget warnings; a high/xhigh "reasoning sandwich".
- Reasoning effort alone: xhigh 53.9 (timeouts) against high 63.6. No per-change ablation.
- An earlier post measured deepagents-cli with Sonnet 4.5 at 44.9 and 40.4 over 2 trials, level with Claude Code's 40.1 in the paper (https://www.langchain.com/blog/evaluating-deepagents-cli-on-terminal-bench-2-0).

**Repo.** https://github.com/langchain-ai/deepagents · MIT · 29,998 stars · pushed 2026-10-07.

### Claude Code (Anthropic)

**Design**, from official write-ups.
- **Minimal SWE-bench scaffold** (https://www.anthropic.com/engineering/swe-bench-sonnet): a persistent bash and `str_replace_editor` (view, create, str_replace, insert). `str_replace` applies "only if there is exactly one match"; absolute paths are required.
- "We actually spent more time optimizing our tools than the overall prompt" (https://www.anthropic.com/engineering/building-effective-agents).
- **Context** (https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents): compaction keeps a summary plus "the five most recently accessed files"; tool-result clearing is the "lightest touch" compaction; subagents return 1,000–2,000-token summaries; CLAUDE.md up front, files retrieved just in time.
- **Best practices** (https://code.claude.com/docs/en/best-practices): give the agent a check it can run; "Bloated CLAUDE.md files cause Claude to ignore your actual instructions"; hooks are deterministic, memory files advisory.
- **LSP**, since 2.0.74 (CHANGELOG): with a code-intelligence plugin (`.lsp.json` mapping a server to file extensions), Claude gets diagnostics after every edit and a definition/references/hover tool. Official plugins exist for 13 languages (https://code.claude.com/docs/en/plugins/code-intelligence).
- **MCP** (https://code.claude.com/docs/en/mcp): outputs over 10,000 tokens warn; the default cap is 25,000, past which a result is saved to a file; tool descriptions are truncated at 2,048 characters; tool search defers tools once their definitions reach 10% of context.

**Results with Claude Code as the harness.**
- **Terminal-Bench 2.0:** below Terminus 2 for every Claude model in Table 2.
- **CORE-Bench Hard:** Opus 4.5 77.78% under Claude Code against 42.22% under CORE-Agent, and 95.5% after manual re-grading; for Opus 4.1 the order reverses, CORE-Agent 51.11 against 42.22 (https://hal.cs.princeton.edu/corebench_hard).
- **METR, HCAST:** Claude Code beat METR's ReAct scaffold for Opus 4.5 in 50.7% of bootstrap samples, a coin flip (https://evals.alignment.org/notes/2026-02-13-measuring-time-horizon-using-claude-code-and-codex/).

**Repo.** https://github.com/anthropics/claude-code · no open-source licence in the API · 149,759 stars · pushed 2026-10-07.

### Codex CLI (OpenAI)

**Design.**
- **Prompt order** ("Unrolling the Codex agent loop", https://openai.com/index/unrolling-the-codex-agent-loop): server instructions; tools; sandbox rules; developer configuration; AGENTS.md, gathered from the git root to the working directory, 32 KiB limit; environment; the user message.
- **Caching:** hits need exact prefixes. MCP tools listed in an inconsistent order caused cache misses; the post reports it as a bug.
- **Compaction:** a server endpoint returning an opaque item (https://developers.openai.com/api/docs/guides/compaction.md).
- **`apply_patch`:** a Lark grammar (`*** Begin Patch`, Add/Update/Delete File, `@@` context, no line numbers). The parser is lenient: it strips the heredoc wrappers GPT-4.1 emits, and matches context exactly, then trimmed, then with punctuation normalised (https://raw.githubusercontent.com/openai/codex/main/codex-rs/apply-patch/src/parser.rs).
- **GPT-4.1 guide** (https://developers.openai.com/cookbook/examples/gpt4-1_prompting_guide): GPT-4.1 was "extensively trained" on this format; three agentic reminders raised internal SWE-bench Verified "by close to 20%", points or relative unstated; planning added 4%, and tools in the API field rather than the prompt 2%.
- **"Harness engineering"** (https://openai.com/index/harness-engineering/): about a million lines written by agents; a ~100-line AGENTS.md that "serves primarily as a map", with docs as the system of record; custom linters whose error messages "inject remediation instructions into agent context".

**Results.**
- Terminal-Bench 2.0: Codex CLI is the best harness for GPT-5 (49.6 against Terminus 2's 35.2) and for GPT-5.2 (62.9 against 54.0).
- METR: Codex beat Triframe for GPT-5 in only 14.5% of bootstrap samples.

**Repo.** https://github.com/openai/codex · Apache-2.0 · Rust · 128,186 stars · pushed 2026-10-07.

### Gemini CLI (Google)

**Design.**
- **The `replace` tool** expects exactly one match, then cascades: exact; flexible: trimmed-line matching that re-applies the target's indentation, keeping relative offsets; regex; fuzzy Levenshtein, gated by a flag; an LLM rewriting the search and replace strings.

  The strategy that won is logged, but no numbers are published (https://raw.githubusercontent.com/google-gemini/gemini-cli/main/packages/core/src/tools/edit.ts).
- **Other defaults** (https://geminicli.com/docs/reference/configuration/, https://geminicli.com/docs/core/subagents/): GEMINI.md, loaded hierarchically and just in time; `context.fileName` can point at AGENTS.md; compression at 50% of context; tool output truncated at 40,000 characters; shadow-git checkpoints before file edits; subagents with a 30-turn default.

**Results.** Terminal-Bench 2.0, Gemini 2.5 Pro: Gemini CLI 19.6 against Terminus 2 32.6.

**Repo.** https://github.com/google-gemini/gemini-cli · Apache-2.0 · 107,249 stars · pushed 2026-10-07.

### LSP-in-the-loop harnesses: OpenCode and Serena

**OpenCode.**
- After every edit it appends "LSP errors detected in this file, please fix:" with errors only (severity 1), at most 20 per file, for the edited file only, not diffed against the pre-edit state (https://raw.githubusercontent.com/anomalyco/opencode/dev/packages/opencode/src/lsp/diagnostic.ts).
- LSP is disabled by default (https://opencode.ai/docs/lsp/). An experimental `lsp` tool offers definition, references and call hierarchy.
- Repo: https://github.com/anomalyco/opencode (formerly sst/opencode) · MIT · 212,197 stars as returned · pushed 2026-10-07.

**Serena** is an MCP server over language servers: `find_symbol`, `find_referencing_symbols`,
`replace_symbol_body`, `insert_after_symbol`, `rename_symbol`. Its only evaluation is the agent judging itself:
Claude Code with Opus 4.6, JetBrains backend, about 20 tasks
(https://oraios.github.io/serena/04-evaluation/030_results/010_cc_on_tianshou.html).
- A cross-file rename: 1 call against 9.
- A 1-line edit: ~550 characters through Serena against ~120 with the built-in edit.
- A 55-line rewrite: ~2,200 against ~4,400.

Repo: https://github.com/oraios/serena · NOASSERTION (third parties say MIT) · 30,084 stars.

**Monitor-guided decoding** (arXiv 2306.10763) uses an LSP during decoding. It lifted SantaCoder-1.1B's
compilation rate from 59.97 to 73.03, beating text-davinci-003 without the monitor. Repo:
https://github.com/microsoft/monitors4codegen · MIT · 279 stars.

### Self-improving and auto-designed harnesses

**Live-SWE-agent** (arXiv 2511.13646). Starts from mini-swe-agent; after each step, a reflection asks
whether to create or revise a tool. Verified, mini-swe-agent against Live:

| Model | mini | Live |
| --- | ---: | ---: |
| GPT-5-Mini | 59.8 | 63.0 |
| GPT-5 | 65.0 | 68.4 |
| Sonnet 4.5 | 70.6 | 75.4 |
| Gemini 3 Pro | 74.2 | 77.4 |

- SWE-bench Pro, Sonnet 4.5: SWE-agent 43.6, Live 45.8.
- 50-task subset: GPT-5-Nano fell from 44.0 to 14.0, because it "fails to understand the goal of creating custom tools and is often stuck in a loop".
- Ablation: no tool creation 62.0, no reflection 64.0, full 76.0 (https://arxiv.org/html/2511.13646v3, re-fetched).

Repo: https://github.com/OpenAutoCoder/live-swe-agent · MIT · 465 stars.

**Darwin Gödel Machine** (arXiv 2505.22954). An archive of self-modified agents, 80 iterations, about $22,000
per SWE-bench run.
- **Results:** SWE-bench 20.0 to 50.0; Polyglot 14.2 to 30.7.
- **Transfer:** the evolved harness took Claude 3.7 Sonnet from 19.0 to 59.5 and o3-mini from 23.0 to 33.0.
- **What it discovered:**
  - "more granular file viewing (by lines) and more precise file editing (by string replacement)";
  - several attempts with a model choosing the best;
  - rejecting patches that touch only tests.

Repo: https://github.com/jennyzzt/dgm · Apache-2.0 · 2,392 stars.

**SICA** (arXiv 2504.15228). The agent edits its own code.
- **What it built:** of its 14 improvements, most are editing tools (smart editor, diff and edit verification), plus three symbol locators (AST, hybrid).
- **Result:** a SWE-bench Verified subset (n=50) rose from 17% to 53%, but swung 0.44, 0.27, 0.53 across consecutive iterations.

Repo: https://github.com/MaximeRobeyns/self_improving_coding_agent · MIT · 405 stars.

**HGM** (arXiv 2510.21614). Verified-60: SICA 50.0, DGM 53.3, HGM 56.7. Repo:
https://github.com/metauto-ai/HGM · Apache-2.0 · 435 stars.

**Meta-Harness** (arXiv 2603.28052). An agent searches over harness code; its main discovery is an
environment snapshot injected before the first turn. Terminal-Bench 2.0, baselines from the leaderboard:
- Haiku 4.5: OpenHands 13.9, Claude Code 27.5, Terminus 2 28.3, Goose 35.5, Meta-Harness 37.6.
- Opus 4.6: Claude Code 58.0, Terminus 2 62.9, Meta-Harness 76.4, ForgeCode 81.8.

**Agentic Harness Engineering** (arXiv 2604.25850). Ten iterations from a bash-only seed.
- Terminal-Bench 2.0 with GPT-5.4: 69.7 to 77.0 pass@1, against Codex CLI at 71.9.
- One component swapped in alone: memory 75.3, tools 73.0, middleware 71.9, system prompt 67.4.
- Gains are non-additive. "Factual harness structure transfers while prose-level strategy does not."
- Cross-family transfer: +5.1 to +10.1 pp. SWE-bench Verified stayed flat, 75.2 to 75.6 (https://arxiv.org/abs/2604.25850, https://arxiv.org/html/2604.25850).

Repo: https://github.com/china-qijizhifeng/agentic-harness-engineering · MIT · 916 stars.

### Test-time scaling and verifiers

| System | Setting | Result | Source |
| --- | --- | --- | --- |
| CodeMonkeys | Verified, Claude 3.5 Sonnet | 57.4% for about $2,300; oracle coverage 69.8%; random pick 45.8%; selector closes about half the gap | https://arxiv.org/html/2501.14723 |
| R2E-Gym, 32B | Verified | each verifier alone 42–43%; hybrid 51.0% Best@26; oracle 64.4%; "less than 20% of tests provide discriminative signal" | https://arxiv.org/html/2504.07164 |
| SWE-Gym | Verified | Best@16 32.0 against oracle Pass@16 42.8 | https://arxiv.org/html/2412.21139 |
| DeepSWE | Verified | 42.2% pass@1 (16 runs); hybrid Best@16 59% | https://www.together.ai/blog/deepswe |
| Trae Agent, N=3 | Verified, Claude 3.7 | 66.40 against oracle 70.0; removing patch pruning −5.57 | https://arxiv.org/html/2507.23370 |
| Kimi-Dev | Verified | 48.0% at 1 patch × 1 test, 60.4% at 40 × 40 | https://arxiv.org/html/2509.23045 |
| Anthropic | Verified | Opus 4 72.5 to 79.4; Sonnet 4 72.7 to 80.2; Claude 3.7 63.7 to 70.3 on n=489 | https://www.anthropic.com/news/claude-4, https://www.anthropic.com/news/claude-3-7-sonnet |

- **Anthropic's selection:** parallel samples, then discard patches that break visible regression tests ("similar to … Agentless"), then rank with a scoring model.
- **Imperfect unit-test verifiers cap accuracy:** "optimal sampling attempts are often fewer than 10" (https://arxiv.org/abs/2411.17501).

## Cross-cutting evidence

### Same model, different harness

| Model | Benchmark | Harness A → harness B | Δ (pp) | Source |
| --- | --- | --- | ---: | --- |
| Claude Opus 4.5 | CORE-Bench Hard | CORE-Agent 42.22 → Claude Code 77.78 | +35.6 | https://hal.cs.princeton.edu/corebench_hard |
| Claude Opus 4.6 | TB 2.0 | Claude Code 58.0 → ForgeCode 81.8 | +23.8 | https://arxiv.org/html/2603.28052 |
| Claude Haiku 4.5 | TB 2.0 | OpenHands 13.3 → mini-swe-agent 29.8 | +16.5 | https://arxiv.org/html/2601.11868v1 |
| Gemini 2.5 Pro | TB 2.0 | Gemini CLI 19.6 → Terminus 2 32.6 | +13.0 | https://arxiv.org/html/2601.11868v1 |
| GPT-5 | TB 2.0 | Terminus 2 35.2 → Codex CLI 49.6 | +14.4 | https://arxiv.org/html/2601.11868v1 |
| GPT-5.2-Codex | TB 2.0 | deepagents-cli 52.8 → engineered 66.5 | +13.7 | https://www.langchain.com/blog/improving-deep-agents-with-harness-engineering |
| Claude Opus 4.1 | SWE-bench Verified Mini | HAL generalist 42.0 → SWE-agent 54.0 | +12.0 | https://arxiv.org/html/2510.11977 |
| GPT-4o | SWE-bench Verified | SWE-agent 23 → Agentless 33.2 | +10.2 | https://epoch.ai/blog/what-skills-does-swe-bench-verified-evaluate |
| Claude Sonnet 4.5 | SWE-bench Pro | SWE-agent 43.6 → Confucius CCA 52.7 | +9.1 | https://arxiv.org/html/2512.10398 |
| GPT-4 Turbo | SWE-bench Lite | shell-only 11.0 → SWE-agent ACI 18.0 | +7.0 | https://arxiv.org/html/2405.15793 |
| Claude Sonnet 4.5 | SWE-bench Verified | mini-swe-agent 70.6 → Anthropic bash+edit 77.2 | +6.6 | swebench.com JSON; anthropic.com/news/claude-sonnet-4-5 |
| Claude Opus 4.5 | TB 2.0 | Claude Code 52.1 → Terminus 2 57.8 | +5.7 | https://arxiv.org/html/2601.11868v1 |
| Claude 4 Sonnet | SWE-bench Verified | mini 64.93 → SWE-agent 66.6 → OpenHands 70.4 | +5.5 | swebench.com leaderboard JSON |
| Claude Sonnet 4.5 | SWE-bench Verified | mini 70.6 → Live-SWE-agent 75.4 | +4.8 | https://arxiv.org/html/2511.13646v3 |
| Claude Sonnet 4.5 | TB 2.0 | four harnesses, 40.1 to 42.8 | 2.7 | https://arxiv.org/html/2601.11868v1 |
| GPT-5-Nano | Verified (50-task subset) | mini 44.0 → Live-SWE-agent 14.0 | −30.0 | https://arxiv.org/html/2511.13646v3 |
| GPT-4 | HumanEval | warming 93.2 ($2.45) → LATS 88.0 ($134.50) | −5.2 | https://arxiv.org/html/2407.01502 |

- **HAL** (https://arxiv.org/html/2510.11977): task-specific scaffolds beat the generalist in 9 of 12 runs on CORE-Bench Hard and 11 of 12 on SWE-bench Verified Mini. Generalists were cheaper in 20 of 24 comparisons, and higher reasoning effort did not raise accuracy in 21 of 36 runs.
- **Epoch** titles a section "Scaffolds matter as much as models", and names API provider choice as the other large factor (https://epoch.ai/gradient-updates/why-benchmarking-is-hard).

### Design choices and their measured effects

| Design choice | Effect size | Source |
| --- | --- | --- |
| Edit command against bash-only editing | +7.7 pp (Lite, single run) | SWE-agent Table 3 |
| Summarised, bounded search against iterative search | +6.0 pp; iterative worse than none | SWE-agent Table 3 |
| 100-line viewer against full file | +5.3 pp | SWE-agent Table 3 |
| Lint gate on edits | +3.0 pp, inside 6-run noise | SWE-agent Table 3 |
| Last 5 observations against full history | +3.0 pp | SWE-agent Table 3 |
| Observation masking against LLM summary | same solve rate (Qwen3-Coder-480B: raw 53.4, masking 54.8, summary 53.8); about half the cost of raw | https://arxiv.org/html/2508.21433 |
| Context management on or off (Confucius) | Sonnet 4: 42.0 → 48.6 (100-task subset) | https://arxiv.org/html/2512.10398 |
| Context editing plus memory tool | +39%; context editing alone +29%; −84% tokens (internal eval) | https://claude.com/blog/context-management |
| Code actions against JSON tool calls | +22 pp, 28% fewer turns (multi-tool); JSON ≥ code on atomic calls | https://arxiv.org/html/2402.01030 |
| Whole file against search/replace, weak open model | 16.4 against 8.0 (Qwen2.5-Coder-32B, polyglot) | aider polyglot YAML |
| Whole file against search/replace, tiny models | 90.0 against 50.1 (Qwen2.5-Coder-0.5B, Dart) | https://arxiv.org/html/2609.05779 |
| udiff against SEARCH/REPLACE (laziness) | 61 against 20 (GPT-4 Turbo) | https://aider.chat/docs/unified-diffs.html |
| Architect/editor split | +3 to +5 pp; editor brings well-formed to 100% | https://aider.chat/2024/09/26/architect.html |
| Viewer/editor subagents | +2.1 pp, −17.9% cost; search/replace 69.4 against adaptive 69.9 | https://arxiv.org/html/2604.26102v1 |
| Training on 21 tool templates | 92.7% average template-following across 5 scaffolds | https://arxiv.org/html/2603.00729v1 |
| Repository graph as plug-in | +2.0 to +2.7 pp on Lite (Agentless, SWE-agent, AutoCodeRover) | https://arxiv.org/html/2410.14684 |
| Graph query language (Cypher) for weak models | Qwen2 1.95 against AutoCodeRover 9.34 | https://arxiv.org/html/2408.03910 |
| Compiler feedback, one round | compile success 0.88 → 0.98; end-to-end 0.69 → 0.78; 5 rounds only 0.79 | https://arxiv.org/html/2512.02567v1 |
| Compiler-driven retrieval and repair | Pass@10 13.04 → 39.13; plateau after 3 rounds | https://arxiv.org/html/2403.16792 |
| Verifier-error debugging phase | +37 of 137 proofs (Verus) | https://arxiv.org/html/2409.13082v3 |
| Self-built tools with reflection | +14 pp with Sonnet 4.5; −30 pp with GPT-5-Nano | Live-SWE-agent Tables 4–5 |
| Harness memory / tools / middleware / prompt | +5.6 / +3.3 / +2.2 / −2.3 pp | AHE Table 3 |
| Environment snapshot before turn 1 | main source of Meta-Harness's gain | https://arxiv.org/html/2603.28052 |
| Regression-test filter for patches | +1.3 pp (Agentless); +3.4 pp (Trae) | Agentless Table 4; Trae Table 2 |
| Generated reproduction tests | +5.0 pp | Agentless Table 4 |
| Hybrid verifier against single verifier | +8 pp at Best@26 | R2E-Gym |
| Three agentic system-prompt reminders | "close to 20%" (unit unstated) | GPT-4.1 guide |
| Tools in the API field against the prompt | +2% | GPT-4.1 guide |
| Tool search against all tools loaded | Opus 4: 49 → 74; Opus 4.5: 79.5 → 88.1 (internal MCP eval) | https://www.anthropic.com/engineering/advanced-tool-use |
| Tool-use input examples | 72% → 90% on complex parameters (internal) | same |
| Programmatic tool calling | −37% tokens | same |
| Retrieved tool subset against all tools | 13.62% → 43.13% (100 MCP servers) | https://arxiv.org/html/2505.03275 |
| Full tool catalogue against domain subset | Claude-3.5: 97.60 → 69.23; GPT-4.1: 98.08 → 94.71 | https://arxiv.org/html/2506.01056 |
| Augmented tool descriptions | +5.85 pp median, +67.46% steps, regressions in 16.67% | https://arxiv.org/abs/2602.14878 |
| Concise against detailed tool response | 72 against 206 tokens (Slack example) | https://www.anthropic.com/engineering/writing-tools-for-agents |
| Tool descriptions loaded on demand | −46.9% tokens in runs calling MCP | https://cursor.com/blog/dynamic-context-discovery |
| Same weights, different provider | Kimi K2-0905 schema accuracy 100% (official) against 71.96% (Together via OpenRouter) | https://github.com/MoonshotAI/K2-Vendor-Verifier |
| Benchmark leakage through git history | MiniMax-M2.5 Commit0: 50% → 18.8% once history was hidden | OpenHands Index blog |

### Where the failures are

- **Tool use, not task logic.**
  - MCP-Atlas: 56.7% of failures are tool usage (selection and parameters), 30.3% task understanding (https://arxiv.org/html/2602.00933v1).
  - LiveMCP-101: semantic parameter errors are 16–25% for strong models and over 40% for small ones (https://arxiv.org/html/2508.15760v1).
  - MCP-Bench: schema compliance and valid tool naming have "largely converged" above 95% (https://arxiv.org/html/2508.20453).
- **Silent failures.**
  - MCPMark: over 80% of gpt-5-high's failures are "implicit", where the run completes with a wrong result (https://arxiv.org/html/2509.24002v1).
  - Diff-based editing by tiny models fails the same way (arXiv 2609.05779).
- **Loops in weak models.**
  - Over 25% of SWE-agent-LM-32B's trajectories hold a repeated sequence of length ≥10, against under 4% for Claude 3.7 (https://arxiv.org/html/2504.21798).
  - SWE-agent: looping on failing edits is 23.4% of unresolved instances ([[editing-robustness]]).
- **Output size.**
  - Accuracy on JSON tool responses falls as they grow, by 7% for GPT-4o and 91% for Mistral-large (https://arxiv.org/html/2510.15955).
  - A single distractor already lowers long-context accuracy (https://www.trychroma.com/research/context-rot).
- **Format constraints are model-dependent.**
  - Text against JSON answers on GSM8K: Claude-3-Haiku 86.5 against 23.4, Gemini-1.5-Flash 89.3 against 89.2 (https://arxiv.org/html/2408.02442).
  - A matched-prompt rebuttal found no penalty (https://blog.dottxt.ai/say-what-you-mean.html).
  - Code wrapped in JSON scored worse than Markdown in aider's test (https://aider.chat/2024/08/14/code-in-json.html).

## Reference repositories worth studying

| Repository | Licence · stars · last push | Why | What to borrow |
| --- | --- | --- | --- |
| https://github.com/SWE-agent/mini-swe-agent | MIT · 8,271 · 2026-10-06 | The field's control arm, about 100 lines | A minimal arm for LotML's harness, in shape not in bash |
| https://github.com/SWE-agent/SWE-agent | MIT · 20,497 · 2026-10-06 | The only published full interface ablation | Ablation protocol; the lint-gate message (error, attempted edit, original) |
| https://github.com/langchain-ai/deepagents | MIT · 29,998 · 2026-10-07 | LotML's harness | Pre-completion check, loop detection, local-context middleware; offload thresholds |
| https://github.com/harbor-framework/harbor | Apache-2.0 · 5,891 · 2026-10-07 | Terminal-Bench runner, task format, Terminus 2 | Task = instruction, image, final-state tests, solution, time limit; ≥5 trials per pair; double-confirmed completion |
| https://github.com/Aider-AI/aider | Apache-2.0 · 49,411 · 2026-05-22 | Per-model format data; repo map; lint display | Uniform-whitespace fallback; lint errors shown inside the enclosing function; "well-formed" as a metric |
| https://github.com/openai/codex | Apache-2.0 · 128,186 · 2026-10-07 | Production Rust harness; apply_patch; AGENTS.md | Lenient patch parsing; deterministic tool order for caching; AGENTS.md precedence and size limit |
| https://github.com/google-gemini/gemini-cli | Apache-2.0 · 107,249 · 2026-10-07 | Edit cascade with re-indentation | Logging which strategy applied each edit, to learn which matcher LotML's `edit` needs |
| https://github.com/anomalyco/opencode | MIT · 212,197 · 2026-10-07 | LSP diagnostics appended to every edit | Errors-only capped report format; an LSP configuration LotML can register |
| https://github.com/oraios/serena | NOASSERTION · 30,084 · 2026-10-06 | Symbol-level MCP editing over LSP | Tool vocabulary (`replace_symbol_body`, `insert_after_symbol`); call-count and payload protocol |
| https://github.com/OpenHands/software-agent-sdk | MIT · 1,208 · 2026-10-07 | Event-sourced state; MCP tools as typed actions; critic | Event log as the trace format; critic-based best-of-n |
| https://github.com/OpenAutoCoder/Agentless | MIT · 2,122 · 2024-12-22 | Pipeline baseline with per-phase ablations | Selection by regression and reproduction tests, then majority vote |
| https://github.com/OpenAutoCoder/live-swe-agent | MIT · 465 · 2026-01-19 | Self-built tools; weak-model collapse | Reflection prompt; the GPT-5-Nano warning |
| https://github.com/jennyzzt/dgm | Apache-2.0 · 2,392 · 2025-08-13 | What evolution found in edit tools | `is_patch_valid` (reject test-only patches); retry-and-select loop |
| https://github.com/china-qijizhifeng/agentic-harness-engineering | MIT · 916 · 2026-08-03 | Component-swap ablation of a harness | The memory/tools/middleware/prompt ablation design |
| https://github.com/R2E-Gym/R2E-Gym | Apache-2.0 · 337 · 2025-07-13 | Hybrid verifiers | Execution-based plus execution-free selection |
| https://github.com/princeton-pli/hal-harness | none · 310 · 2026-07-01 | Cost-aware multi-scaffold evaluation | A cost and accuracy frontier per model and scaffold |
| https://github.com/MoonshotAI/K2-Vendor-Verifier | not checked | Provider conformance for tool calls | A tool-call conformance check per OpenRouter provider before a run |
| https://github.com/github/github-mcp-server | MIT · 33,433 · 2026-10-07 | Toolsets, consolidation, read-only mode | Profiles exposing fewer tools; consolidation (Projects tools −50% tokens) |

AutoCodeRover is left out as a source to borrow from: its licence restricts use.

## Implications for LotML

### Agent harness (deepagents over OpenRouter)

1. **Make the harness an experimental variable.**
   - Add a minimal arm: a small loop of OpenRouter tool calls over the compiler's MCP tools only, with no planning tool, subagents, summarisation or built-in file tools.
   - The evidence says a heavier harness can halve or zero a weak model's score: Haiku 4.5 scored 13.3 under OpenHands against 29.8 under mini-swe-agent, and GPT-5-Nano 14 against 44.
   - LotML's first model, `z-ai/glm-5.3-flash`, is a cheap one. Without the minimal arm, a failure cannot be pinned on LotML rather than on deepagents.
   - ADR 0014 already allows "a further arm".
2. **Pin the OpenRouter provider and record it.**
   - Tool-call schema accuracy for the same weights ranges from 71.96% to 100% across providers (K2-Vendor-Verifier), and Epoch ranks provider choice next to scaffold.
   - The only graded agent row so far records `"providers": {}`: the provider is not being captured. Pin one provider per model for a comparison, and fail a run whose response names another.
   - OpenRouter's `:exacto` variants route to providers with better tool-call quality. Its blog gives no number in readable text; the "10–20%" figure is search-snippet only, UNVERIFIED.
3. **Fix the sampling protocol.**
   - Temperature 0 with repeated attempts measures provider nondeterminism, not the model's distribution, so pass@k from such runs misleads. Sample at temperature >0 for pass@k; best-of-n systems use 1.0.
   - Report pass^k (all k succeed) beside pass@k, as Anthropic's eval guidance does and MCPMark reports pass^4.
   - Run each task and arm at least 5 times, as Terminal-Bench does.
4. **Add the cheap middleware that was measured, one piece at a time.**
   - A pre-completion check: `check` and `test` must run before the agent may finish.
   - Loop detection: the same diagnostic code N times, or N edits of one symbol, triggers a nudge.
   - A start-of-run snapshot: the `digest` in the first message.
   - Each was part of a measured gain (deepagents +13.7 combined; Meta-Harness's main discovery; AHE middleware +2.2). None has a published single-change ablation, so measure each as an arm.
5. **Remove overlapping tools.**
   - The agent sees deepagents' `edit_file`, `write_file`, `read_file` and `grep` beside the compiler's `edit`, `replace`, `show` and `references`, all unprefixed.
   - Wrong-tool selection between near-synonyms is the most common failure Anthropic reports, and tool usage is 56.7% of MCP-Atlas failures.
   - Either prefix the compiler's tools (`lotml_edit`, …) or deny the built-in edit tools on `.lot` and `.lotml` files, then measure which the model uses. The one passing run used `read_file` 6 times, the compiler's `replace` twice and `edit` once.
6. **Watch context costs.** That run spent 142,182 input tokens over 13 model calls, about 11k per call.
   - Keep the tool list and its order fixed for prompt caching: Codex's cache bug; Manus's cached input at a tenth of the price; MCP 2026-07-28 asking for deterministic order.
   - Keep any tool result under deepagents' 20,000-token offload threshold: an offloaded `digest` becomes a 10-line preview.
7. **Scale the task set.**
   - Eight tasks cannot show a 13-point harness effect. The HumanEval and MBPP agent tasks already converted (157 and 859 kept) reach the 168 paired tasks [[evaluation-harness]] gives for 10 pp.
   - Harness effects of 5–24 pp are measurable at 84–249 pairs. A 3 pp effect like the lint gate is not affordable, so prioritise the large-effect knobs.
8. **Benchmark hygiene, mostly in place already.**
   - Fresh workspace copies carry no git history (the Commit0 leak), and hidden tests stay out of the workspace.
   - Grading is on final state, as Terminal-Bench grades.
   - Still to add: impossible tasks for reward hacking ([[rl-environment]]).

### Compiler tooling (JSON diagnostics, symbol-addressed edits, MCP)

1. **Symbol- and content-addressed edits are the consensus.**
   - Every self-improving harness converged on them, OpenAI and aider removed line numbers, and CODESTRUCT is already in the wiki.
   - Keep both `replace` (a whole unit) and `edit` (line text). Whole-unit rewriting is what weak open models do well: Qwen2.5-Coder-32B 16.4 against 8.0, tiny models 90 against 50. LotML's editing test already saw open models answer with whole functions.
   - Serena's numbers show a symbol tool costs more than a string edit for a one-line change, so measure tool choice per model rather than forcing one.
2. **Tolerant matching is industry practice.** aider (a uniform offset), Gemini CLI (re-applied indentation) and Codex (trim passes) all do what LotML's `edit` does. Log which matching strategy applied each edit, as Gemini CLI does, to learn whether tolerance is ever needed; the editing pilot never needed it.
3. **Reporting only the diagnostics an edit introduced is LotML's untested distinction.**
   - Claude Code and OpenCode append the current errors after each edit; OpenCode's are errors only, at most 20 per file, with no baseline. The only measured gate is SWE-agent's +3 pp, a single run.
   - Run the ablation: no report, all current errors, introduced only. That is publishable evidence nobody has.
4. **Bounded, root-cause-first diagnostics fit the evidence:** SWE-agent's bounded search and window; accuracy falling as JSON tool responses grow; 2–3 repair rounds capturing most of the gain. Keep five by default.
5. **Write compact descriptions and add examples.**
   - Each tool description should state purpose, limits and parameter formats, with the critical text in the first 2,048 characters, where Claude Code truncates.
   - Add one or two input examples to `replace`, `add`, `edit` and `rename` (Anthropic internal: 72% to 90%). Full augmentation cost 67% more steps.
6. **Shape the results.**
   - Keep `isError: true` with actionable text for refused edits, as the 2025-11-25 spec now asks for input errors.
   - Consider `structuredContent` with an `outputSchema` for harnesses, plus a compact text block. The spec says the text SHOULD carry serialized JSON; the 2026-07-28 examples use a prose summary.
   - Add `limit`/`offset` or a concise mode to `digest` and `references`, so no response approaches Claude Code's 10k-token warning or deepagents' 20k offload.
7. **Tool count.** Fourteen compiler tools plus about nine deepagents built-ins is above OpenAI's soft "fewer than 20" and below the 30–50 where Claude degrades; MCP-Zero shows the sensitivity depends on the model. Offer a lean profile (check, digest, show, replace, edit, test) and measure it against the full set, after GitHub's toolsets and Block's workflow-level tools.
8. **Ship into the harnesses people use.** `lotml init` registers MCP for Claude Code, Codex and Cursor. Two cheap additions:
   - a Claude Code code-intelligence plugin entry (`.lsp.json` mapping `lotml lsp` to `.lot` and `.lotml`), which gives diagnostics after every edit without MCP;
   - an OpenCode `lsp` entry.

   Codex and Claude Code also lean on CLIs ("the most context-efficient way"), so `AGENTS.md` should keep the `lotml check --json` and `test --json` commands, not only the MCP tools. At 49 lines it is within OpenAI's "~100-line map".
9. **Use the compiler as the free selector for best-of-n.** Selection, not generation, limits best-of-n: the coverage gaps are 10–14 pp. `lotml check` plus the visible `test` blocks is the Agentless and Anthropic rejection filter at no model cost. Measure pass@k (coverage) against selected@k, with and without the filter.

### What to measure, in order of expected effect

1. **Harness arm.** Compare deepagents, a minimal MCP loop, and later Claude Code or Codex headless, on the same tasks, model and provider. Use ≥5 samples at temperature >0, paired per task, with exact McNemar on majority outcomes. Report pass@1, pass^k and cost.
2. **Provider.** Run the same model through two pinned OpenRouter providers, and count tool-call parse errors and schema failures per provider.
3. **Check-on-edit report.** Compare none, all errors and introduced only. Record edit failures, recovery after a failed edit (SWE-agent's 90.5% to 57.2%) and rounds to green.
4. **Edit tools exposed.** Compare compiler symbol edits only, text edits only, both, and both plus deepagents `edit_file`. Record apply failures, well-formed rate per model and lines changed.
5. **Context.** Compare `digest` preloaded against on demand, and the full tool set against the lean profile. Record input tokens per call and cache hits.
6. **Middleware.** Measure the pre-completion check and loop detection, one at a time.
7. **Best-of-n with the check-plus-test filter:** coverage against selection.
8. **Process metrics on every run:**
   - repeated action sequences ≥10, as SWE-smith counts them;
   - failed edits and steps to the first `check`;
   - finishes declared with failing visible tests;
   - tool errors by tool;
   - timeouts and step limits.

### Conflicts with ADRs

- **adr:0014-deepagents-over-openrouter-for-the-agent-harness.** No conflict, but three pressures:
  - its "a further arm rather than a replacement" is now required by the evidence, not optional;
  - it makes provider pinning optional, while the provider-variance evidence argues for pinning by default when comparing. That changes a consequence of the ADR, so it needs a spec delta or a superseding ADR;
  - bash-first harnesses (Terminus, mini-swe-agent) would conflict with its no-shell, no-container posture (and specs/agent-harness R2.2). A shell arm needs a new ADR with a container.
- **adr:0017 (the guide model).** No conflict with its choice of model or runtime. The evidence warns that helper agents and tools can be net zero or negative: Augment's regression-fixing agent was net zero, Refact's critique tool hurt, and self-built tools sank a weak driver. The guide's benefit must be measured as an arm, which specs/guide-evaluation/ already plans.
- **adr:0009 and adr:0010 (indentation with symbol-addressed edits).** Supported: self-improving harnesses converged on precise, addressed edits plus verification.
- **No ADR is contradicted.** One wiki statement needs correcting: Claude Code's after-edit LSP diagnostics need a code-intelligence plugin.

## Sources

Papers:
- **Harnesses and agents:** SWE-agent: https://arxiv.org/html/2405.15793; CodeAct: https://arxiv.org/html/2402.01030; OpenHands: https://arxiv.org/html/2407.16741; OpenHands SDK: https://arxiv.org/html/2511.03690; Agentless: https://arxiv.org/html/2407.01489; AutoCodeRover: https://arxiv.org/html/2404.05427; SWE-Search: https://arxiv.org/html/2410.20285; Trae Agent: https://arxiv.org/html/2507.23370; Confucius Code Agent: https://arxiv.org/html/2512.10398; SWE-smith: https://arxiv.org/html/2504.21798
- **Evaluation of harnesses:** Terminal-Bench: https://arxiv.org/html/2601.11868v1; HAL: https://arxiv.org/html/2510.11977; AI Agents That Matter: https://arxiv.org/html/2407.01502; SWE-Bench Pro: https://arxiv.org/html/2509.16941
- **Self-improving harnesses:** Live-SWE-agent: https://arxiv.org/html/2511.13646v3; DGM: https://arxiv.org/html/2505.22954; SICA: https://arxiv.org/html/2504.15228; HGM: https://arxiv.org/html/2510.21614; Meta-Harness: https://arxiv.org/html/2603.28052; AHE: https://arxiv.org/abs/2604.25850; Harness Engineering, a source study of 11 harnesses: https://arxiv.org/abs/2609.00006
- **Context and editing:** Complexity Trap: https://arxiv.org/html/2508.21433; SWE-Edit: https://arxiv.org/abs/2604.26102; Qwen3-Coder-Next: https://arxiv.org/html/2603.00729v1; diffs against whole files: https://arxiv.org/html/2609.05779; EDIT-Bench: https://arxiv.org/html/2511.04486v2
- **Compiler, LSP and structure:** MGD: https://arxiv.org/html/2306.10763; RepoGraph: https://arxiv.org/html/2410.14684; LocAgent: https://arxiv.org/html/2503.09089; CodexGraph: https://arxiv.org/html/2408.03910; CoCoGen: https://arxiv.org/html/2403.16792; C-to-Rust loop: https://arxiv.org/html/2512.02567v1; AutoVerus: https://arxiv.org/html/2409.13082v3
- **Test-time scaling:** CodeMonkeys: https://arxiv.org/html/2501.14723; R2E-Gym: https://arxiv.org/html/2504.07164; SWE-Gym: https://arxiv.org/html/2412.21139; Kimi-Dev: https://arxiv.org/html/2509.23045; limits of resampling: https://arxiv.org/abs/2411.17501; o1 system card: https://arxiv.org/html/2412.16720; learning to verify: https://arxiv.org/abs/2603.03800
- **Tools and MCP:** RAG-MCP: https://arxiv.org/html/2505.03275; MCP-Zero: https://arxiv.org/html/2506.01056; MCP tool descriptions: https://arxiv.org/abs/2602.14878; MCP-Atlas: https://arxiv.org/html/2602.00933v1; LiveMCP-101: https://arxiv.org/html/2508.15760v1; MCP-Bench: https://arxiv.org/html/2508.20453; MCPMark: https://arxiv.org/html/2509.24002v1; tool outputs: https://arxiv.org/html/2510.15955; format restrictions: https://arxiv.org/html/2408.02442

Vendor and project write-ups:
- **Anthropic:** building effective agents: https://www.anthropic.com/engineering/building-effective-agents; SWE-bench with Claude 3.5 Sonnet: https://www.anthropic.com/engineering/swe-bench-sonnet; context engineering: https://www.anthropic.com/engineering/effective-context-engineering-for-ai-agents; writing tools for agents: https://www.anthropic.com/engineering/writing-tools-for-agents; advanced tool use: https://www.anthropic.com/engineering/advanced-tool-use; code execution with MCP: https://www.anthropic.com/engineering/code-execution-with-mcp; evals for agents: https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents; multi-agent research system: https://www.anthropic.com/engineering/multi-agent-research-system; context management: https://claude.com/blog/context-management; model announcements: https://www.anthropic.com/news/claude-sonnet-4-5, https://www.anthropic.com/news/claude-4, https://www.anthropic.com/news/claude-3-7-sonnet
- **Claude Code docs:** best practices: https://code.claude.com/docs/en/best-practices; code intelligence: https://code.claude.com/docs/en/plugins/code-intelligence; LSP tool behaviour: https://code.claude.com/docs/en/tools-reference#lsp-tool-behavior; MCP: https://code.claude.com/docs/en/mcp
- **OpenAI:** the Codex agent loop: https://openai.com/index/unrolling-the-codex-agent-loop; harness engineering: https://openai.com/index/harness-engineering/; GPT-4.1 prompting guide: https://developers.openai.com/cookbook/examples/gpt4-1_prompting_guide; function calling: https://developers.openai.com/api/docs/guides/function-calling; AGENTS.md: https://agents.md/
- **Google:** Gemini CLI configuration: https://geminicli.com/docs/reference/configuration/
- **LangChain:** harness engineering: https://www.langchain.com/blog/improving-deep-agents-with-harness-engineering; deepagents-cli on Terminal-Bench 2.0: https://www.langchain.com/blog/evaluating-deepagents-cli-on-terminal-bench-2-0; context engineering: https://docs.langchain.com/oss/python/deepagents/context-engineering
- **Aider:** edit formats: https://aider.chat/docs/more/edit-formats.html; unified diffs: https://aider.chat/docs/unified-diffs.html; architect/editor: https://aider.chat/2024/09/26/architect.html; polyglot benchmark: https://aider.chat/2024/12/21/polyglot.html; leaderboards: https://aider.chat/docs/leaderboards/; SWE-bench Lite: https://aider.chat/2024/05/22/swe-bench-lite.html; code in JSON: https://aider.chat/2024/08/14/code-in-json.html
- **OpenHands:** critic: https://openhands.dev/blog/sota-on-swe-bench-verified-with-inference-time-scaling-and-critic-model; OpenHands Index: https://openhands.dev/blog/analyzing-and-improving-openhands-index
- **Leaderboards and evaluators:** SWE-bench roulette: https://www.swebench.com/post-250820-mini-roulette.html; Epoch, SWE-bench skills: https://epoch.ai/blog/what-skills-does-swe-bench-verified-evaluate; Epoch, why benchmarking is hard: https://epoch.ai/gradient-updates/why-benchmarking-is-hard; HAL, CORE-Bench Hard: https://hal.cs.princeton.edu/corebench_hard; METR: https://evals.alignment.org/notes/2026-02-13-measuring-time-horizon-using-claude-code-and-codex/
- **Others:** DeepSWE: https://www.together.ai/blog/deepswe; Cursor, dynamic context discovery: https://cursor.com/blog/dynamic-context-discovery; Cursor, instant apply: https://cursor.com/blog/instant-apply; Manus: https://manus.im/blog/Context-Engineering-for-AI-Agents-Lessons-from-Building-Manus; Chroma, context rot: https://www.trychroma.com/research/context-rot; Block, MCP servers: https://engineering.block.xyz/blog/blocks-playbook-for-designing-mcp-servers; OpenRouter, Exacto: https://openrouter.ai/blog/announcements/provider-variance-introducing-exacto/; K2 Vendor Verifier: https://github.com/MoonshotAI/K2-Vendor-Verifier; Augment: https://www.augmentcode.com/blog/1-open-source-agent-on-swe-bench-verified-by-combining-claude-3-7-and-o1
- **MCP specification:** https://modelcontextprotocol.io/specification/2025-11-25/server/tools, https://modelcontextprotocol.io/specification/2026-07-28/server/tools
