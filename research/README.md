# research

Medições do estudo da linguagem X. A interpretação está no wiki: `docs/wiki/pages/custo-em-tokens.md`
e `docs/wiki/pages/piloto-de-vazamento-de-python.md`.

- `tokens/` — contador multi-tokenizador (`counter.py`), corpus pareado (`corpus/`, medido byte a
  byte: não formatar) e `measure.py`, que gera `results.md` e `results.json`.
- `pilot/` — specs das variantes A e B, tarefas, gramática Lark e detector de vazamento
  (`check.py`), verificador semântico (`semantics.py`), programas gerados (`runs/`) e
  `analyze.py`, que gera `results.md` e `results.json`.

```bash
# tokens: testes (a primeira execução baixa tokenizadores do Hugging Face) e medição
cd research/tokens
uv run --with pytest --with tiktoken --with tokenizers --with huggingface_hub pytest
uv run --with tiktoken --with tokenizers --with huggingface_hub python measure.py

# piloto: testes e análise
cd research/pilot
uv run --with pytest --with lark pytest
uv run --with lark python analyze.py

# lint e formatação
uvx ruff check research && uvx ruff format --check research
```
