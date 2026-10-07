<p align="center">
  <img src=".github/assets/banner.png" alt="LotML — a glowing pixel-art lotus with a code chevron at its heart, on a pond at night" width="100%">
</p>

<h1 align="center">
  <img src=".github/assets/icon.png" alt="" width="48">
  LotML
</h1>

<p align="center">
  <strong>A programming language designed to be written by LLMs and reviewed by people.</strong>
</p>

<p align="center">
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-blue.svg"></a>
  <img alt="Compiler in Rust" src="https://img.shields.io/badge/compiler-Rust%202024-orange.svg">
  <img alt="Target: Python, C in progress" src="https://img.shields.io/badge/target-Python%20%C2%B7%20C%20in%20progress-teal.svg">
  <img alt="Status: research preview" src="https://img.shields.io/badge/status-research%20preview-yellow.svg">
  <img alt="File extension .lot" src="https://img.shields.io/badge/extension-.lot-8a2be2.svg">
</p>

---

LotML is a statically typed language with Python's syntax wherever its meaning is Python's — and a
visible difference wherever it is not. Its name is Lot, from Lotus, and ML, from Machine Learning;
its files are `.lot`, and `.lotml` is accepted too. It exists so that coding agents write correct
code more often, and so that the people reviewing that code can trust what they read:

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

[Releases](https://github.com/protonspy/lotml/releases) hold `lotml` built for Linux (x86_64),
Windows (x86_64) and macOS (Apple silicon): each archive holds the binary, this README and the
licence, beside the VS Code extension's `.vsix` and a `SHA256SUMS`. The binary carries its
runtimes; running programs needs Python 3.11 or later, and the C target a C compiler.

To build it instead, with Rust 1.97+:

```bash
cargo build --manifest-path compiler/Cargo.toml --locked --release -p lotml
```

```bash
lotml check stats.lot       # syntax, type and mutability errors
lotml test  stats.lot       # run the test blocks
lotml run   main.lot        # run fn main()
lotml build stats.lot       # compile to a Python module with its runtime
lotml init                  # set a project up for coding agents: AGENTS.md, guide, MCP server
lotml explain E0204         # explain an error code
```

### File icons and VS Code

The lotus on a page marks LotML files in Windows Explorer, for the current user, with no
administrator needed:

```powershell
powershell -ExecutionPolicy Bypass -File editors\windows\register.ps1   # -Remove takes it back
```

The VS Code extension in [`editors/vscode/`](editors/vscode/) gives LotML files the same icon and
starts the language server for them; its README says how to build and install it.

## Repository

| Path | What lives there |
|---|---|
| [`compiler/`](compiler/) | The Rust compiler: syntax, checker, formatter, Python backend, LSP and MCP servers |
| [`reference/`](reference/) | The language reference and the tree-sitter grammar |
| [`editors/`](editors/) | The file icon, its Windows Explorer association and the VS Code extension |
| [`harness/`](harness/) | The evaluation harness that measures models writing LotML |
| [`research/`](research/) | Reproducible experiments: token cost, Python leakage, editing robustness |
| [`docs/`](docs/) | Knowledge base: wiki, ADRs, glossary, stack |
| [`plans/`](plans/) · [`specs/`](specs/) | The roadmap and the feature specs it is built from |

## Status

LotML is a research project, built phase by phase, each phase closed by a measured gate
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

## References

The design rests on 83 papers, downloaded and checked: 1,047 quotes, each found verbatim in its
source by [`research/literature/verify.py`](research/literature/verify.py). How the check works and
what it corrected is in [source verification](docs/wiki/pages/source-verification.md); the list
with versions and SHA-256 is [`research/literature/sources.json`](research/literature/sources.json).

<details>
<summary>All 83 papers, newest first</summary>

- Dmytro Ustynov (2026). *Beyond Human-Readable: Rethinking Software Engineering Conventions for the Agentic Development Era*. [arXiv:2604.07502](https://arxiv.org/abs/2604.07502) — 13 quotes
- Alexandru-Radu Moraru et al. (2026). *Beyond the Traceback: Using LLMs for Adaptive Explanations of Programming Errors*. [arXiv:2608.20896](https://arxiv.org/abs/2608.20896) — 10 quotes
- Chen Shen et al. (2026). *Bridging the Knowledge Void: Inference-time Acquisition of Unfamiliar Programming Languages for Coding Tasks*. [arXiv:2602.06976](https://arxiv.org/abs/2602.06976) — 17 quotes
- Junhang Cheng et al. (2026). *CangjieBench: Benchmarking LLMs on a Low-Resource General-Purpose Programming Language*. [arXiv:2603.14501](https://arxiv.org/abs/2603.14501) — 12 quotes
- Myeongsoo Kim et al. (2026). *CODESTRUCT: Code Agents over Structured Action Spaces*. [arXiv:2604.05407](https://arxiv.org/abs/2604.05407) — 11 quotes
- Pengcheng Xu (2026). *Does a Language Server Save Tokens for Coding Agents? A Measurement Methodology and Preliminary Study*. [arXiv:2608.13568](https://arxiv.org/abs/2608.13568) — 17 quotes
- Aman Sharma and Paras Chopra (2026). *EsoLang-Bench: Evaluating Genuine Reasoning in Large Language Models via Esoteric Programming Languages*. [arXiv:2603.09678](https://arxiv.org/abs/2603.09678) — 13 quotes
- Aman Sharma et al. (2026). *Frontier Coding Agents Use Metaprogramming to Adapt to Unfamiliar Programming Languages*. [arXiv:2606.10933](https://arxiv.org/abs/2606.10933) — 10 quotes
- Niels Mündler-Sasahara et al. (2026). *Generative Compilation: On-the-Fly Compiler Feedback as AI Generates Code*. [arXiv:2607.13921](https://arxiv.org/abs/2607.13921) — 13 quotes
- Johin Johny Arimbur (2026). *How Many Tries Does It Take? Iterative Self-Repair in LLM Code Generation Across Model Scales and Benchmarks*. [arXiv:2604.10508](https://arxiv.org/abs/2604.10508) — 11 quotes
- Alessandro Schena et al. (2026). *Large Language Models and Language Server Protocol: a match made in context*. [arXiv:2609.03086](https://arxiv.org/abs/2609.03086) — 10 quotes
- Alessandro Giagnorio et al. (2026). *No Resource, No Benchmarks, No Problem? Evaluating and Improving LLMs for Code Generation in No-Resource Languages*. [arXiv:2606.16827](https://arxiv.org/abs/2606.16827) — 17 quotes
- Nguyet-Anh H. Lang et al. (2026). *Perish or Flourish? A Holistic Evaluation of Large Language Models for Code Generation in Functional Programming*. [arXiv:2601.02060](https://arxiv.org/abs/2601.02060) — 9 quotes
- Louis Lalonde et al. (2026). *Revisiting Feedback-Driven LLM Code Repair: A Replication and Exploratory Java Extension*. [arXiv:2609.00362](https://arxiv.org/abs/2609.00362) — 8 quotes
- Paul Kronlund-Drouault (2026). *Semantic Prefix Oracles for LLM Decoding: Contracts and Differential Validation*. [arXiv:2609.35425](https://arxiv.org/abs/2609.35425) — 12 quotes
- Jaideep Ray et al. (2026). *Structured Feedback Improves Repair in an LLM Agent Loop*. [arXiv:2607.14167](https://arxiv.org/abs/2607.14167) — 12 quotes
- Vinayshekhar Bannihatti Kumar et al. (2026). *Syntax Without Semantics: Teaching Large Language Models to Code in an Unseen Language*. [arXiv:2605.15607](https://arxiv.org/abs/2605.15607) — 13 quotes
- Matteo Biagiola et al. (2026). *The Alignment Problem in Constrained Code Generation*. [arXiv:2606.21619](https://arxiv.org/abs/2606.21619) — 20 quotes
- Zixuan Wu et al. (2026). *The Best Programming Language for Tokenmaxxing: An Investigation of Coding Agent Behavior Across Programming Languages*. [arXiv:2607.22807](https://arxiv.org/abs/2607.22807) — 24 quotes
- Wei Cheng et al. (2026). *To Diff or Not to Diff? Structure-Aware and Adaptive Output Formats for Efficient LLM-based Code Editing*. [arXiv:2604.27296](https://arxiv.org/abs/2604.27296) — 11 quotes
- Shriram Krishnamurthi and Matthew Flatt (2026). *Type-Error Ablation and AI Coding Agents*. [arXiv:2606.01522](https://arxiv.org/abs/2606.01522) — 20 quotes
- Rodrigo Pato Nogueira et al. (2026). *Unreliable in Practice? A Comprehensive Study of Errors in LLM-Generated Code*. [arXiv:2608.00661](https://arxiv.org/abs/2608.00661) — 15 quotes
- Zike Li et al. (2025). *A Preliminary Study on the Robustness of Code Generation by Large Language Models*. [arXiv:2503.20197](https://arxiv.org/abs/2503.20197) — 9 quotes
- Aleksander Boruch-Gruszecki et al. (2025). *Agnostics: Learning to Code in Any Programming Language via Reinforcement with a Universal Learning Environment*. [arXiv:2508.04865](https://arxiv.org/abs/2508.04865) — 15 quotes
- Saif Khalfan Saif Al Mazrouei (2025). *Anka: A Domain-Specific Language for Reliable LLM Code Generation*. [arXiv:2512.23214](https://arxiv.org/abs/2512.23214) — 10 quotes
- Debangshu Banerjee et al. (2025). *CRANE: Reasoning with constrained LLM generation*. [arXiv:2502.09061](https://arxiv.org/abs/2502.09061) — 14 quotes
- Evgeniy Glukhov et al. (2025). *Diff-XYZ: A Benchmark for Evaluating Diff Understanding*. [arXiv:2510.12487](https://arxiv.org/abs/2510.12487) — 9 quotes
- Saibo Geng et al. (2025). *JSONSchemaBench: A Rigorous Benchmark of Structured Outputs for Language Models*. [arXiv:2501.10868](https://arxiv.org/abs/2501.10868) — 15 quotes
- Aditya Thimmaiah et al. (2025). *LLMs Lean on Priors, Not Programming Language Semantics*. [arXiv:2510.03415](https://arxiv.org/abs/2510.03415) — 10 quotes
- Lorenzo Lee Solano et al. (2025). *Narrowing the Gap: Supervised Fine-Tuning of Open-Source LLMs as a Viable Alternative to Proprietary Models for Pedagogical Tools*. [arXiv:2507.05305](https://arxiv.org/abs/2507.05305) — 10 quotes
- Behnam Mohammadi (2025). *Pel, A Programming Language for Orchestrating AI Agents*. [arXiv:2505.13453](https://arxiv.org/abs/2505.13453) — 11 quotes
- Micheline Bénédicte Moumoula et al. (2025). *Programming Language Confusion: When Code LLMs Can't Keep their Languages Straight*. [arXiv:2503.13620](https://arxiv.org/abs/2503.13620) — 13 quotes
- Stephen Mell et al. (2025). *Quasar: A Programming Language Specialized for LLM Code Actions*. [arXiv:2506.12202](https://arxiv.org/abs/2506.12202) — 14 quotes
- David Jiahao Fu et al. (2025). *SLMFix: Leveraging Small Language Models for Domain Specific Language Error Fixing with Reinforcement Learning*. [arXiv:2511.19422](https://arxiv.org/abs/2511.19422) — 10 quotes
- Muntasir Adnan and Carlos C. N. Kuhn (2025). *The Debugging Decay Index: Rethinking Debugging Strategies for Code LLMs*. [arXiv:2506.18403](https://arxiv.org/abs/2506.18403) — 9 quotes
- Dangfeng Pan et al. (2025). *The Hidden Cost of Readability: How Code Formatting Silently Consumes Your LLM Budget*. [arXiv:2508.13666](https://arxiv.org/abs/2508.13666) — 11 quotes
- Zhensu Sun et al. (2025). *Token Sugar: Making Source Code Sweeter for LLMs through Token-Efficient Shorthand*. [arXiv:2512.08266](https://arxiv.org/abs/2512.08266) — 11 quotes
- Niels Mündler et al. (2025). *Type-Constrained Code Generation with Language Models*. [arXiv:2504.09246](https://arxiv.org/abs/2504.09246) — 22 quotes
- Chunqiu Steven Xia et al. (2024). *Agentless: Demystifying LLM-based Software Engineering Agents*. [arXiv:2407.01489](https://arxiv.org/abs/2407.01489) — 12 quotes
- Zhensu Sun et al. (2024). *AI Coders Are Among Us: Rethinking Programming Language Grammar Towards Efficient Code Generation*. [arXiv:2404.16333](https://arxiv.org/abs/2404.16333) — 21 quotes
- Manushree Vijayvergiya et al. (2024). *AI-Assisted Assessment of Coding Practices in Modern Code Review*. [arXiv:2405.13565](https://arxiv.org/abs/2405.13565) — 10 quotes
- Anton Semenkin et al. (2024). *Full Line Code Completion: Bringing AI to Desktop*. [arXiv:2405.08704](https://arxiv.org/abs/2405.08704) — 10 quotes
- Kanghee Park et al. (2024). *Grammar-Aligned Decoding*. [arXiv:2405.21047](https://arxiv.org/abs/2405.21047) — 7 quotes
- Xufeng Yao et al. (2024). *HDLdebugger: Streamlining HDL debugging with Large Language Models*. [arXiv:2403.11671](https://arxiv.org/abs/2403.11671) — 10 quotes
- Kyle Wong et al. (2024). *Investigating the Transferability of Code Repair for Low-Resource Programming Languages*. [arXiv:2406.14867](https://arxiv.org/abs/2406.14867) — 10 quotes
- Peiyang Song et al. (2024). *Lean Copilot: Large Language Models as Copilots for Theorem Proving in Lean*. [arXiv:2404.12534](https://arxiv.org/abs/2404.12534) — 10 quotes
- Zhi Rui Tam et al. (2024). *Let Me Speak Freely? A Study on the Impact of Format Restrictions on Performance of Large Language Models*. [arXiv:2408.02442](https://arxiv.org/abs/2408.02442) — 14 quotes
- Maciej Pankiewicz and Ryan S. Baker (2024). *Navigating Compiler Errors with AI Assistance -- A Study of GPT Hints in an Introductory Programming Course*. [arXiv:2403.12737](https://arxiv.org/abs/2403.12737) — 10 quotes
- Eddie Antonio Santos and Brett A. Becker (2024). *Not the Silver Bullet: LLM-enhanced Programming Error Messages are Ineffective in Practice*. [arXiv:2409.18661](https://arxiv.org/abs/2409.18661) — 10 quotes
- Nam Le Hai et al. (2024). *On the Impacts of Contexts on Repository-Level Code Generation*. [arXiv:2406.11927](https://arxiv.org/abs/2406.11927) — 11 quotes
- Guangsheng Ou et al. (2024). *RustRepoTrans: Repository-level Code Translation Benchmark Targeting Rust*. [arXiv:2411.13990](https://arxiv.org/abs/2411.13990) — 16 quotes
- Andrew Blinn et al. (2024). *Statically Contextualizing Large Language Models with Typed Holes*. [arXiv:2409.00921](https://arxiv.org/abs/2409.00921) — 14 quotes
- John Yang et al. (2024). *SWE-agent: Agent-Computer Interfaces Enable Automated Software Engineering*. [arXiv:2405.15793](https://arxiv.org/abs/2405.15793) — 23 quotes
- Shubham Ugare et al. (2024). *SynCode: LLM Generation with Grammar Augmentation*. [arXiv:2403.01632](https://arxiv.org/abs/2403.01632) — 15 quotes
- Federico Mora et al. (2024). *Synthetic Programming Elicitation for Text-to-Code in Very Low-Resource Programming and Formal Languages*. [arXiv:2406.03636](https://arxiv.org/abs/2406.03636) — 14 quotes
- Kunhao Zheng et al. (2024). *What Makes Large Language Models Reason in (Multi-Turn) Code Generation?*. [arXiv:2410.08105](https://arxiv.org/abs/2410.08105) — 10 quotes
- Shihan Dou et al. (2024). *What's Wrong with Your Code Generated by Large Language Models? An Extensive Study*. [arXiv:2407.06153](https://arxiv.org/abs/2407.06153) — 9 quotes
- Yixin Dong et al. (2024). *XGrammar: Flexible and Efficient Structured Generation Engine for Large Language Models*. [arXiv:2411.15100](https://arxiv.org/abs/2411.15100) — 11 quotes
- Saurabh Pujar et al. (2023). *Automated Code generation for Information Technology Tasks in YAML through Large Language Models*. [arXiv:2305.02783](https://arxiv.org/abs/2305.02783) — 10 quotes
- Andrew Taylor et al. (2023). *Dcc --help: Generating Context-Aware Compiler Error Explanations with Large Language Models*. [arXiv:2308.11873](https://arxiv.org/abs/2308.11873) — 10 quotes
- Brandon T. Willard and Rémi Louf (2023). *Efficient Guided Generation for Large Language Models*. [arXiv:2307.09702](https://arxiv.org/abs/2307.09702) — 6 quotes
- Pantazis Deligiannis et al. (2023). *Fixing Rust Compilation Errors using LLMs*. [arXiv:2308.05177](https://arxiv.org/abs/2308.05177) — 17 quotes
- Harshit Joshi et al. (2023). *FLAME: A small language model for spreadsheet formulas*. [arXiv:2301.13779](https://arxiv.org/abs/2301.13779) — 10 quotes
- Nalin Wadhwa et al. (2023). *Frustrated with Code Quality Issues? LLMs can Help!*. [arXiv:2309.12938](https://arxiv.org/abs/2309.12938) — 10 quotes
- Tung Phung et al. (2023). *Generating High-Precision Feedback for Programming Syntax Errors using Large Language Models*. [arXiv:2302.04662](https://arxiv.org/abs/2302.04662) — 10 quotes
- Lakshya A Agrawal et al. (2023). *Guiding Language Models of Code with Global Context using Monitors*. [arXiv:2306.10763](https://arxiv.org/abs/2306.10763) — 11 quotes
- Theo X. Olausson et al. (2023). *Is Self-Repair a Silver Bullet for Code Generation?*. [arXiv:2306.09896](https://arxiv.org/abs/2306.09896) — 9 quotes
- Federico Cassano et al. (2023). *Knowledge Transfer from High-Resource to Low-Resource Programming Languages for Code LLMs*. [arXiv:2308.09895](https://arxiv.org/abs/2308.09895) — 16 quotes
- Yun-Da Tsai et al. (2023). *RTLFixer: Automatically Fixing RTL Syntax Errors with Large Language Models*. [arXiv:2311.16543](https://arxiv.org/abs/2311.16543) — 10 quotes
- Xinyun Chen et al. (2023). *Teaching Large Language Models to Self-Debug*. [arXiv:2304.05128](https://arxiv.org/abs/2304.05128) — 14 quotes
- Dimitri Racordon et al. (2022). *Implementation Strategies for Mutable Value Semantics*. [PDF](https://www.jot.fm/issues/issue_2022_02/article2.pdf) — 21 quotes
- Federico Cassano et al. (2022). *MultiPL-E: A Scalable and Extensible Approach to Benchmarking Neural Code Generation*. [arXiv:2208.08227](https://arxiv.org/abs/2208.08227) — 18 quotes
- Rohan Bavishi et al. (2022). *Neurosymbolic Repair for Low-Code Formula Languages*. [arXiv:2207.11765](https://arxiv.org/abs/2207.11765) — 10 quotes
- Harshit Joshi et al. (2022). *Repair Is Nearly Generation: Multilingual Program Repair with LLMs*. [arXiv:2208.11640](https://arxiv.org/abs/2208.11640) — 10 quotes
- Juho Leinonen et al. (2022). *Using Large Language Models to Enhance Programming Error Messages*. [arXiv:2210.11630](https://arxiv.org/abs/2210.11630) — 10 quotes
- Michihiro Yasunaga and Percy Liang (2021). *Break-It-Fix-It: Unsupervised Learning for Program Repair*. [arXiv:2106.06600](https://arxiv.org/abs/2106.06600) — 11 quotes
- Alex Reinking et al. (2021). *Perceus: Garbage Free Reference Counting with Reuse*. [PDF](https://xnning.github.io/papers/perceus.pdf) — 20 quotes
- Toufique Ahmed et al. (2021). *SYNFIX: Automatically Fixing Syntax Errors using Compiler Diagnostics*. [arXiv:2104.14671](https://arxiv.org/abs/2104.14671) — 13 quotes
- Michihiro Yasunaga and Percy Liang (2020). *Graph-based, Self-Supervised Program Repair from Diagnostic Feedback*. [arXiv:2005.10636](https://arxiv.org/abs/2005.10636) — 11 quotes
- Sebastian Ullrich and Leonardo de Moura (2019). *Counting Immutable Beans: Reference Counting Optimized for Purely Functional Programming*. [arXiv:1908.05647](https://arxiv.org/abs/1908.05647) — 18 quotes
- Jiho Choi et al. (2018). *Biased Reference Counting: Minimizing Atomic Operations in Garbage Collection*. [PDF](https://iacoma.cs.uiuc.edu/iacoma-papers/pact18.pdf) — 13 quotes
- Michael Pradel and Koushik Sen (2018). *DeepBugs: A Learning Approach to Name-based Bug Detection*. [arXiv:1805.11683](https://arxiv.org/abs/1805.11683) — 10 quotes
- Baishakhi Ray et al. (2015). *On the "Naturalness" of Buggy Code*. [arXiv:1506.01159](https://arxiv.org/abs/1506.01159) — 11 quotes

</details>

## Topics

`programming-language` · `compiler` · `rust` · `python` · `llm` · `ai-agents` · `code-generation` ·
`static-typing` · `transpiler` · `language-design` · `mcp` · `lsp`

#lotml #ProgrammingLanguages #LLM #AIAgents #Compilers #RustLang #Python #LanguageDesign
#CodeGeneration #OpenSource

## License

[MIT](LICENSE)
