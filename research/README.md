# research

Measurements for the lotml study. The pilot and the corpus use the provisional name X and the
`.x` extension, as the models saw them. The interpretation lives in the wiki:
`docs/wiki/pages/token-cost.md` and `docs/wiki/pages/python-leakage-pilot.md`.

- `tokens/` — multi-tokenizer counter (`counter.py`), paired corpus (`corpus/`, measured byte for
  byte: do not format it) and `measure.py`, which writes `results.md` and `results.json`.
- `pilot/` — specs of variants A and B, tasks, the Lark grammar and leakage detector
  (`check.py`), the semantic checker (`semantics.py`), the generated programs (`runs/`) and
  `analyze.py`, which writes `results.md` and `results.json`.

Pinned versions and revisions: the packages below and the tokenizer commits in
`tokens/counter.py`.

```bash
# tokens: tests (the first run downloads tokenizers from Hugging Face) and measurement
cd research/tokens
uv run --with pytest==9.1.1 --with tiktoken==0.14.0 --with tokenizers==0.23.2 --with huggingface_hub==1.33.0 pytest
uv run --with tiktoken==0.14.0 --with tokenizers==0.23.2 --with huggingface_hub==1.33.0 python measure.py

# pilot: tests and analysis
cd research/pilot
uv run --with pytest==9.1.1 --with lark==1.3.1 pytest
uv run --with lark==1.3.1 python analyze.py

# lint and formatting
uvx ruff check research && uvx ruff format --check research
```
