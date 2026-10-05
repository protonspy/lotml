# Stack

Every adopted technology, with one line on why it earned its place. Technology not listed here
is an open decision, never something adopted silently. The language's own toolchain does not
exist yet; it is decided in `docs/adr/` and enters this list when code depends on it.

## Pesquisa (`research/`)

- **uv** — roda os scripts de pesquisa com dependências efêmeras (`uv run --with …`), sem manifesto de projeto para manter.
- **tiktoken** — tokenizadores `o200k` e `cl100k` da OpenAI, para as medições de tokens.
- **tokenizers** e **huggingface_hub** — carregam os tokenizadores abertos medidos (Llama 3, Qwen3, DeepSeek-V3, Gemma 3, Mistral Nemo, StarCoder2).
- **lark** — gramática LALR das variantes A e B no piloto, com indentação pelo `Indenter`.
- **pytest** — testes dos contadores e verificadores de pesquisa.
- **ruff** — lint e formatação do código de pesquisa; `research/ruff.toml` exclui o corpus, que é medido byte a byte.
