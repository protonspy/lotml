# research

Medições do estudo da linguagem X. A interpretação está no wiki: `docs/wiki/pages/custo-em-tokens.md`
e `docs/wiki/pages/piloto-de-vazamento-de-python.md`.

- `tokens/` — contador multi-tokenizador (`counter.py`), corpus pareado (`corpus/`, medido byte a
  byte: não formatar) e `measure.py`, que gera `results.md` e `results.json`.
- `pilot/` — specs das variantes A e B, tarefas, gramática Lark e detector de vazamento
  (`check.py`), verificador semântico (`semantics.py`), programas gerados (`runs/`) e
  `analyze.py`, que gera `results.md` e `results.json`.

Versões e revisões fixadas: os pacotes abaixo e os commits dos tokenizadores em
`tokens/counter.py`.

```bash
# tokens: testes (a primeira execução baixa tokenizadores do Hugging Face) e medição
cd research/tokens
uv run --with pytest==9.1.1 --with tiktoken==0.14.0 --with tokenizers==0.23.2 --with huggingface_hub==1.33.0 pytest
uv run --with tiktoken==0.14.0 --with tokenizers==0.23.2 --with huggingface_hub==1.33.0 python measure.py

# piloto: testes e análise
cd research/pilot
uv run --with pytest==9.1.1 --with lark==1.3.1 pytest
uv run --with lark==1.3.1 python analyze.py

# lint e formatação
uvx ruff check research && uvx ruff format --check research
```
