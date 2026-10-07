# research

Measurements for the lotml study. The pilot and the corpus use the provisional name X and the
`.x` extension, as the models saw them. The interpretation lives in the wiki:
`docs/wiki/pages/token-cost.md`, `python-leakage-pilot.md`, `editing-robustness.md`,
`source-verification.md` and the pages they link.

- `tokens/` — multi-tokenizer counter (`counter.py`), paired corpus (`corpus/`, measured byte for
  byte: do not format it) and `measure.py`, which writes `results.md` and `results.json`.
- `pilot/` — specs of variants A and B, tasks, the Lark grammar and leakage detector
  (`check.py`), the semantic checker (`semantics.py`), the generated programs (`runs/`) and
  `analyze.py`, which writes `results.md` and `results.json`.
- `literature/` — the sources the study rests on (`sources.json`, with URL and checksum), a fetcher
  that downloads each PDF and converts it to Markdown with docling (`fetch.py`, into the
  git-ignored `cache/`), and a verifier (`verify.py`) that checks every quote in `claims.json`
  against the converted text and writes `results.md`.
- `experiments/` — one directory per question, each with tests and a script that writes
  `results.md`:
  - `tracebacks/` — a Python AST carrying lotml positions, and the tracebacks it produces;
  - `overflow/` — the cost of trapping and wrapping `i64` arithmetic on CPython;
  - `sample_size/` — McNemar sample sizes against the exact test's power;
  - `transpiler/` — a research transpiler from variants A and B to Python that runs the pilot's
    `test` blocks under lotml's semantics and under Python's (`pilot.py`);
  - `indentation/` — indentation slips against brace slips over the corpus and the pilot;
  - `grammar/` — block-structure grammars for constrained decoding, checked with llguidance;
  - `editing/` — the editing pilot: tasks with hidden tests, the prompts the models read
    (`prompt/`), their answers (`runs/`) and the scorer (`editing.py`).
- `prior-art/` — sixteen compilers and interpreters of Python and its derivatives, read against
  lotml's compiler: one study per project (`studies/`), the synthesis (`synthesis.md`) and the
  commits read (`README.md`). It runs nothing; the clones it cites are git-ignored, and are read,
  never built.
- `llm-landscape/` — three sourced reports: languages built for models (`languages.md`), coding
  agent harnesses (`harnesses.md`), and evaluation with new-language adaptation
  (`evaluation-and-adaptation.md`). Their quotations came through an extraction step and are not yet
  in `literature/claims.json`.

**Running model output executes it.** `transpiler/`, `indentation/` and `editing/` run
model-written programs and edits in-process. The transpiler refuses imports outside `math` and
any dunder name and exposes only a short list of built-ins, and tests run under a line budget,
but none of that is a sandbox: run a new, unreviewed batch of model output in a container or a
throwaway machine. `literature/claims.json` holds short attributed quotations from each paper,
on purpose; the papers themselves stay in the ignored `cache/`.

Pinned versions and revisions: the packages below and the tokenizer commits in
`tokens/counter.py`. Transitive dependencies are not locked.

```bash
# tokens: tests (the first run downloads tokenizers from Hugging Face) and measurement
cd research/tokens
uv run --with pytest==9.1.1 --with tiktoken==0.14.0 --with tokenizers==0.23.2 --with huggingface_hub==1.33.0 pytest
uv run --with tiktoken==0.14.0 --with tokenizers==0.23.2 --with huggingface_hub==1.33.0 python measure.py

# pilot: tests and analysis
cd research/pilot
uv run --with pytest==9.1.1 --with lark==1.3.1 pytest
uv run --with lark==1.3.1 python analyze.py

# literature: tests, download and conversion (the first run downloads docling's models), checking
cd research/literature
uv run --with pytest==9.1.1 pytest
uv run --with docling==2.133.0 python fetch.py
uv run python verify.py

# experiments without third-party packages: tracebacks, overflow, sample_size
cd research/experiments/tracebacks && uv run --with pytest==9.1.1 pytest && uv run python tracebacks.py
cd research/experiments/overflow && uv run --with pytest==9.1.1 pytest && uv run python overflow.py
cd research/experiments/sample_size && uv run --with pytest==9.1.1 pytest && uv run python sample_size.py

# experiments on the pilot's grammar: transpiler, indentation, editing
cd research/experiments/transpiler && uv run --with pytest==9.1.1 --with lark==1.3.1 pytest && uv run --with lark==1.3.1 python pilot.py
cd research/experiments/indentation && uv run --with pytest==9.1.1 --with lark==1.3.1 pytest && uv run --with lark==1.3.1 python slips.py
cd research/experiments/editing && uv run --with pytest==9.1.1 --with lark==1.3.1 pytest && uv run --with lark==1.3.1 python editing.py

# grammar: llguidance and the o200k tokenizer
cd research/experiments/grammar
uv run --with pytest==9.1.1 --with lark==1.3.1 --with llguidance==1.9.1 --with tiktoken==0.14.0 pytest
uv run --with lark==1.3.1 --with llguidance==1.9.1 --with tiktoken==0.14.0 python grammars.py

# lint and formatting
uvx ruff check research && uvx ruff format --check research
```
