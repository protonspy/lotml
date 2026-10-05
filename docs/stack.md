# Stack

Every adopted technology, with one line on why it earned its place. Technology not listed here
is an open decision, never something adopted silently. The language's own toolchain does not
exist yet; it is decided in `docs/adr/` and enters this list when code depends on it.

## Research (`research/`)

- **uv** — runs the research scripts with ephemeral dependencies (`uv run --with …`), with no project manifest to maintain.
- **tiktoken** — OpenAI's `o200k` and `cl100k` tokenizers, for the token measurements.
- **tokenizers** and **huggingface_hub** — load the open tokenizers measured (Llama 3, Qwen3, DeepSeek-V3, Gemma 3, Mistral Nemo, StarCoder2).
- **lark** — the LALR grammar of variants A and B in the pilot, with indentation through `Indenter`.
- **pytest** — tests for the research counters and checkers.
- **ruff** — lint and formatting for the research code; `research/ruff.toml` excludes the corpus, which is measured byte for byte.
