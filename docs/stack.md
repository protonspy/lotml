# Stack

Every adopted technology, with one line on why it earned its place. Technology not listed here
is an open decision, never something adopted silently. Decisions that are hard to reverse are in
`docs/adr/`; this list is what the code depends on.

## Research (`research/`)

- **uv** — runs the research scripts with ephemeral dependencies (`uv run --with …`), with no project manifest to maintain.
- **tiktoken** — OpenAI's `o200k` and `cl100k` tokenizers, for the token measurements and as llguidance's tokenizer.
- **tokenizers** and **huggingface_hub** — load the open tokenizers measured (Llama 3, Qwen3, DeepSeek-V3, Gemma 3, Mistral Nemo, StarCoder2).
- **lark** — the LALR grammar of variants A and B in the pilot, with indentation through `Indenter`.
- **docling** — converts the downloaded papers to Markdown page by page, tables included, so quotes can be checked against them (`research/literature/`).
- **llguidance** — the constrained-decoding engine behind OpenAI's grammar tools, llama.cpp, vLLM and SGLang; checks which grammars of lotml it accepts and what each costs per token (`research/experiments/grammar/`).
- **pytest** — tests for the research counters and checkers.
- **ruff** — lint and formatting for the research code; `research/ruff.toml` excludes the corpus, which is measured byte for byte.

## Evaluation harness (`harness/`)

A uv project with a lock file, because the harness is a deliverable rather than a one-off
measurement; it reuses the research stack above (lark, tiktoken, llguidance, pytest, ruff) at the
same pinned versions, and tokenizers with huggingface_hub to count a guidance record against the
base model's context with its own tokenizer (specs/guide-records/ R2.6), in a `guide` dependency
group the default environment leaves out.

- **pytest-cov** — line coverage for the test gate `scc check` reads; chosen over running
  `coverage` by hand because it attaches to the pytest run the suite already is.

- **Ollama** and **OpenRouter** (services, called over HTTP with the standard library) — serve the
  open models the experiments ask. Ollama runs them on the local machine; OpenRouter's raw text
  completions run hosted open-weight models from one provider pinned per model, checked to take
  the prompt as written, when a local run would take hours. The key is read from
  `OPENROUTER_API_KEY` and never written to the repository.

- **deepagents** 0.7 (group `agent`) — the coding agent loop of the agent harness: planning, file
  tools confined to a workspace, context management; chosen over writing the loop, which is not
  what lotml studies (adr:0014-deepagents-over-openrouter-for-the-agent-harness). It brings
  LangChain and LangGraph.
- **langchain-openrouter** 0.2 (group `agent`) — the chat model with tool calls deepagents needs,
  over OpenRouter, with its provider routing; the raw completions above stay on the standard
  library.

- **tree-sitter CLI** 0.27 (development only, through `npx`) — generates the editor grammar in
  `reference/grammar/tree-sitter/` and parses the corpus with it in the harness tests, which
  skip without `npx`; it compiles the parser with the platform's C compiler.

## The harness guide (`harness/`, group `train`, adr:0016-a-half-billion-coder-model-tuned-locally-and-served-by-llama-server)

Outside the default environment: `uv run --group train` trains the guide on the local GPU.

- **torch** 2.11.0 built for CUDA 12.8, from PyTorch's own index — the training runtime on the
  local RTX 3060.
- **transformers** 5.19.0, **peft** 0.21.2, **trl** 1.0.0 — the base model, its LoRA adapter, and
  supervised fine-tuning with the loss on the answer alone.
- **datasets** 5.1.0 and **accelerate** 1.15.0 — what trl reads its examples through and trains
  with.
- **llama.cpp** (build b11450, a release binary and its source's `convert_hf_to_gguf.py`, used by
  path) — converts the merged guide to a Q4_K_M GGUF, and `llama-server` serves it on the CPU with
  JSON-schema answers and log-probabilities, the request the `guide` tool sends.
- **Qwen/Qwen2.5-Coder-0.5B-Instruct** (revision `ea3f2471`, Apache-2.0) — the base model; its
  tokenizer counts the guidance records.

## Compiler (`compiler/`)

A Cargo workspace, Rust 1.97 and edition 2024 (adr:0006-compiler-written-in-rust), with a lock
file. Each crate is a stage: `lotml-syntax` (lexer, tolerant parser), `lotml-diag` (diagnostics
and their codes), `lotml-check` (types, mutability, errors as values), `lotml-db` (the queries),
`lotml-fmt` (the formatter), `lotml-py` (the Python backend and its runtime), `lotml-ide` (what
each name refers to, and the workspace an editor or agent queries), `lotml` (the command, with
its language and MCP servers — JSON-RPC written over `serde_json`, with no protocol library).

- **salsa** 0.28 — incremental queries over source files, the property the under-100 ms check
  rests on ([[transpilation-strategy]]); chosen over a hand-rolled cache because rust-analyzer
  runs on it. Costs: it labels itself experimental, so its API may move under us.
- **serde** and **serde_json** — the versioned JSON of diagnostics and test reports, and the
  syntax tree the Python backend hands its runtime; no other format is read or written.
- **clap** 4 — the command line, derived from the `Command` enum so help text and arguments
  cannot drift apart; chosen over hand parsing for its error messages.
- **cargo-llvm-cov** (development only) — line coverage of the Rust tests for the test gate,
  next to the harness's pytest-cov; it needs the `llvm-tools-preview` component.
- **typeshed** (read, not linked) — the stubs `lotml bind` writes interfaces from
  (adr:0012-python-interop-through-checked-boundaries-and-interface-files); found in an installed
  mypy or jedi when no `--stub` is given, never installed by lotml.
- **CPython** 3.11 or later — runs what the Python backend writes (`lotml run`, `lotml test`);
  found as `LOTML_PYTHON`, `python3`, `python` or `py -3`. Not a library the compiler links.
