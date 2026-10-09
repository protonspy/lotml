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

- **mypy**, **types-requests**, **types-PyYAML**, **types-python-dateutil**, **types-six**,
  **pandas-stubs**, **numpy**, **urllib3**, **packaging**, **idna**, **certifi** and
  **charset-normalizer** (group `stubs`, pinned) — the stubs of the binding coverage corpus,
  read and never imported: typeshed's `stdlib` from mypy's wheel, the others' from their stub
  distributions or their own types
  (adr:0030-the-binding-coverage-report-reads-stubs-from-pinned-distributions).

- **tree-sitter CLI** 0.27 (development only, through `npx`) — generates the editor grammar in
  `reference/grammar/tree-sitter/` and parses the corpus with it in the harness tests, which
  skip without `npx`; it compiles the parser with the platform's C compiler.

## The harness guide (`harness/`, group `train`, adr:0017-a-half-billion-coder-model-tuned-locally-and-served-by-llama-server)

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
`lotml-fmt` (the formatter), `lotml-ir` (the IR every backend reads, its lowering and its native
passes, adr:0020-one-ir-between-the-checker-and-every-backend), `lotml-py` (the Python backend,
writing its module from the generic IR, and its runtime), `lotml-llvm` (the LLVM backend, the C
library export and its `clang` driver,
adr:0025-two-targets-python-for-run-llvm-for-build), `lotml-runtime` (the C runtime native programs
run on, adr:0016-c-target-as-monomorphic-c-over-a-counting-runtime), `lotml-ide` (what each name
refers to, and the workspace an editor or agent queries), `lotml-bind` (a Python stub read as an
interface, and the typeshed stubs lotml carries,
adr:0032-a-python-module-is-bound-at-check-time-from-stubs-read-in-rust), `lotml` (the command,
with its language and MCP servers — JSON-RPC written over `serde_json`, with no protocol library).

- **salsa** 0.28 — incremental queries over source files, the property the under-100 ms check
  rests on ([[transpilation-strategy]]); chosen over a hand-rolled cache because rust-analyzer
  runs on it. Costs: it labels itself experimental, so its API may move under us.
- **serde** and **serde_json** — the versioned JSON of diagnostics and test reports, and the
  syntax tree the Python backend hands its runtime; no other format is read or written.
- **toml** 1.1 — reads a project's `uv.lock` and `pyproject.toml`, so a lock is checked to name
  PyPI alone before uv installs from it
  (adr:0033-lotml-run-installs-a-project-s-python-dependencies-from-its-uv-lock); the parser
  Cargo reads its own manifests with, chosen over reading the lock line by line, which a reformatted
  file would walk around.
- **clap** 4 — the command line, derived from the `Command` enum so help text and arguments
  cannot drift apart; chosen over hand parsing for its error messages.
- **cargo-llvm-cov** (development only) — line coverage of the Rust tests for the test gate,
  next to the harness's pytest-cov; it needs the `llvm-tools-preview` component.
- **cargo-fuzz** 0.13.2 and **libfuzzer-sys** 0.4 (development only, on nightly) — coverage-guided
  fuzzing of the parser, the checker and both targets' lowering, as ruff fuzzes its parser, in
  `compiler/fuzz/` outside the workspace so the compiler stays on stable
  (adr:0028-fuzz-the-frontend-with-cargo-fuzz-on-nightly); libFuzzer does not link with MSVC, so on
  Windows it runs under WSL.
- **typeshed** (read, not linked) — the stubs `lotml bind` writes interfaces from
  (adr:0012-python-interop-through-checked-boundaries-and-interface-files): its `stdlib` vendored
  as text at the commit `compiler/crates/lotml-bind/typeshed/COMMIT` pins, with its licence, and
  embedded in lotml (adr:0032); `release/typeshed.py <commit>` moves the pin.
- **ruff_python_parser** and **ruff_python_ast** 0.0.16, pinned exactly — the Python parser
  `lotml-bind` reads a stub with, so binding runs no Python (adr:0032). MIT. Costs: Ruff publishes
  them as internal crates with no stable API, so a version bump is a change of its own.
- **miniz_oxide** — deflates the vendored stubs at build time and inflates them on first use, about
  0.5 MB in the binary for 4.6 MB of text (adr:0032); pure Rust, the deflate Rust's own toolchain
  uses.
- **CPython** 3.14 — runs what the Python backend writes (`lotml run`, `lotml test`), found in
  the order adr:0026-lotml-ships-uv-and-runs-python-3-14-by-default sets: `LOTML_PYTHON`, the
  project's virtual environment for `run` and `test`, the one uv installed, a download through
  uv, and with no uv `python3`, `python` or `py -3` (3.11 or later). Not a library the compiler
  links. The harness provisions it once ahead of a run (`python -m lotml_harness.python
  --provision`) and runs offline.
- **uv** — shipped beside the binary in each release archive, pinned by version and SHA-256 in
  `release/uv.json`, with its licences; lotml finds it by absolute path (`LOTML_UV`, beside
  itself, then the path's absolute entries) and runs it from its own cache with uv's
  configuration ignored, to provision CPython 3.14 and, from a project's `uv.lock`, its
  dependencies (adr:0026, adr:0033-lotml-run-installs-a-project-s-python-dependencies-from-its-uv-lock).
- **clang** 17 or later — compiles the LLVM IR the LLVM backend writes, with the C runtime, into
  an executable, or with `--shared` a shared library
  (`--target llvm`, adr:0021-compiler-in-rust-with-llvm-as-its-native-code-generator):
  found as `LOTML_CLANG`, `clang` on `PATH`, then where the LLVM installer for Windows puts it.
  Not a library the compiler links; textual IR keeps the compiler's own build free of LLVM's
  libraries. On Windows it links with Visual Studio's linker and libraries.
- **llvm-mingw** (Windows), **clang** and **lld** (Linux), **clang** with Apple's `ld` (macOS),
  provisioned on the first native build from this repository's toolchain release, pinned by hash
  (adr:0027-lotml-provisions-a-pinned-llvm-toolchain-on-first-build). Decided, and built by
  `plans/native-toolchain.md`; until then the clang entry above describes the code.
- **llvm-dwarfdump**, **llvm-objdump**, **llvm-readobj** and **nm** (development only) — what the
  LLVM target's tests read a line table and a library's exported symbols with, found beside
  `clang` or on `PATH`; CI checks it has `llvm-dwarfdump`.

## Editors (`editors/`)

- **Node.js** 20 or later — builds and tests the VS Code extension (`npm ci`, `node --test`);
  the harness test that runs those tests skips without `node`.
- **vscode-languageclient** 10.1.2 — the VS Code extension's client of `lotml lsp`, Microsoft's
  own implementation of the protocol for VS Code; the extension's one dependency.
- **@vscode/vsce** 4.0.0 (development only, a pinned dev dependency in the lockfile) — packages
  the extension as the `.vsix` it is installed from; nothing is published, so the install script
  of `@vscode/vsce-sign`, which signs for publishing, is denied.
- **Windows PowerShell** 5.1 (part of Windows) — runs `editors/windows/register.ps1`, which gives
  `.lot` and `.lotml` their icon in Explorer; chosen over a `lotml` subcommand, which would link a
  registry crate into every build for something done once per machine.

## Delivery (`.github/workflows/`)

- **GitHub Actions** — CI on every push and pull request (`ci.yml`), and the release, started by
  hand from `main` (`release.yml`): `lotml` built on GitHub's Linux, Windows and macOS runners and
  published with the VS Code extension as a GitHub release. Every action is pinned to a commit.
