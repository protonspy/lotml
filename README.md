<p align="center">
  <img src=".github/assets/banner.png" alt="lotml — a robot writes indented code, a person reviews it, a green check between them" width="100%">
</p>

<h1 align="center">
  <img src=".github/assets/icon.png" alt="" width="48">
  lotml
</h1>

<p align="center">
  <strong>A programming language designed to be written by LLMs and reviewed by people.</strong>
</p>

<p align="center">
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <img alt="Compiler in Rust" src="https://img.shields.io/badge/compiler-Rust%202024-orange.svg">
  <img alt="Target: Python, C in progress" src="https://img.shields.io/badge/target-Python%20%C2%B7%20C%20in%20progress-teal.svg">
  <img alt="Status: research preview" src="https://img.shields.io/badge/status-research%20preview-yellow.svg">
  <img alt="File extension .lotml" src="https://img.shields.io/badge/extension-.lotml-8a2be2.svg">
</p>

---

lotml is a statically typed language with Python's syntax wherever its meaning is Python's — and a
visible difference wherever it is not. It exists so that coding agents write correct code more
often, and so that the people reviewing that code can trust what they read:

- **Python's vocabulary where the semantics match**, so a model's training prior helps instead of
  misleading it.
- **Static types checkable on prefixes**, while the code is still being written.
- **Errors as values**, not exceptions — every failure path is in the signature.
- **Value semantics** with reference counting: no aliasing surprises.
- **A compiler built as the agent's tool** — `check`, `explain`, `digest`, `show`, an LSP server
  and an MCP server, all answering in versioned JSON.
- **`test` blocks in the language**, reporting the values a failed comparison saw.

Every design decision is backed by a measurement or a checked source, not an opinion: see
[the study](docs/wiki/pages/llm-oriented-language.md) and the
[decision records](docs/adr/).

## A taste

```
fn median(xs: [f64]) -> f64?:
    """The middle value of `xs` once sorted; None when `xs` is empty."""
    if len(xs) == 0:
        return None
    var s = xs
    s.sort()
    mid = len(s) // 2
    if len(s) % 2 == 1:
        return s[mid]
    return (s[mid - 1] + s[mid]) / 2.0

test "median of three":
    assert median([3.0, 1.0, 2.0]) == 2.0
```

`var s = xs` copies — `xs` is never mutated behind the caller's back. `f64?` is an optional the
checker forces you to handle. The full language fits in one example-driven page:
[`reference/lotml.md`](reference/lotml.md).

## Getting started

Requires Rust 1.97+ and Python 3 (the first target compiles to Python modules).

```bash
cargo build --manifest-path compiler/Cargo.toml --locked --release -p lotml
```

```bash
lotml check stats.lotml     # syntax, type and mutability errors
lotml test  stats.lotml     # run the test blocks
lotml run   main.lotml      # run fn main()
lotml build stats.lotml     # compile to a Python module with its runtime
lotml init                  # set a project up for coding agents: AGENTS.md, guide, MCP server
lotml explain E0204         # explain an error code
```

## Repository

| Path | What lives there |
|---|---|
| [`compiler/`](compiler/) | The Rust compiler: syntax, checker, formatter, Python backend, LSP and MCP servers |
| [`reference/`](reference/) | The language reference and the tree-sitter grammar |
| [`harness/`](harness/) | The evaluation harness that measures models writing lotml |
| [`research/`](research/) | Reproducible experiments: token cost, Python leakage, editing robustness |
| [`docs/`](docs/) | Knowledge base: wiki, ADRs, glossary, stack |
| [`plans/`](plans/) · [`specs/`](specs/) | The roadmap and the feature specs it is built from |

## Status

lotml is a research project, built phase by phase, each phase closed by a measured gate
([roadmap](plans/lotml-roadmap.md)):

- [x] **Phase 0** — syntax settled by measurement (variant B, significant indentation)
- [x] **Phase 1** — v1 on the Python target (gate missed on pass@1 and overridden: [adr 0011](docs/adr/0011-proceed-to-phase-2-past-the-failed-phase-1-gate.md))
- [x] **Phase 2** — agent tooling: LSP, MCP, symbol-addressed edits, Python interop, colorless concurrency, C FFI, tree-sitter grammar
- [ ] **Phase 3** — the C target, within 2× C on numeric code *(in progress)*
- [ ] **Phase 4** — a native backend (Cranelift, LLVM), effects as capabilities, `where` contracts

## Development

```bash
uv --directory harness run pytest
cargo test --manifest-path compiler/Cargo.toml
cargo clippy --manifest-path compiler/Cargo.toml --locked --all-targets -- -D warnings
```

The project is spec-driven; `CLAUDE.md` and `.claude/rules/` describe the workflow.

## Topics

`programming-language` · `compiler` · `rust` · `python` · `llm` · `ai-agents` · `code-generation` ·
`static-typing` · `transpiler` · `language-design` · `mcp` · `lsp`

#lotml #ProgrammingLanguages #LLM #AIAgents #Compilers #RustLang #Python #LanguageDesign
#CodeGeneration #OpenSource

## License

[MIT](LICENSE)
