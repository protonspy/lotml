# Source verification

The first study checked each claim against its source by reading; this page describes how the
second pass made that check repeatable, what it found, and what it cannot guarantee. The machinery
is in `research/literature/`; the claims it checks are used throughout the wiki, starting from
[[llm-oriented-language]].

## How it works

- **Sources** (`sources.json`): 83 papers — the 43 the first survey cited plus 15 published since or
  missed by it, found by a search on edit formats, languages designed for LLMs, prefix checking,
  diagnostics and low-resource languages, and 25 on a small model inside the compiler
  ([[compiler-embedded-model]]). Each has a versioned URL and the SHA-256 of the PDF.
- **Fetch** (`fetch.py`): downloads each PDF into the git-ignored `cache/` — papers are identified,
  not redistributed — and converts it to Markdown page by page with docling, tables included.
- **Claims** (`claims.json`): every number or statement the study uses, with a verbatim quote from
  the converted paper. Three kinds: a *claim* the wiki makes, a *discrepancy* where the paper says
  something other than the first survey, and a *finding* the first survey did not have.
- **Verify** (`verify.py`): canonicalizes text — ligatures, case, hyphens split across lines,
  punctuation and whitespace folded away; digits, decimal points and the signs and comparisons
  beside a digit kept, so `-5%` cannot match `5%` nor `p < 0.05` match `p > 0.05` — and requires
  each quote to occur in its paper, reporting the page. A quote shorter than 12 canonical characters is
  refused; within one paper, longer strings do not match by accident.

The quotes were extracted by seventeen reading passes, each covering four to six papers in full —
abstract, method, every results section and table, limitations — with instructions to check the
denominator, model, benchmark and condition behind every number. The verifier then checked every
quote mechanically.

## What it found

1,047 quotes, all found verbatim: 546 claims, 112 discrepancies and 389 findings over 83 papers.

The discrepancies are the first survey's errors, and most are not wrong numbers but numbers
detached from their conditions — one model, one benchmark, a relative gain read as points, a best
case read as typical. The ones that changed a conclusion:

| what the first survey said | what the paper says | where it matters |
| --- | --- | --- |
| compile errors are 94.8% of failures translating to Rust | 92.3% in the cited version | [[semantic-compiler]] |
| unresolved names, imports and traits dominate Rust errors | missing methods lead translation; type mismatches (43.4%) lead self-contained Rust, traits 6.1% | [[semantic-compiler]], [[memory-model]] |
| 94% of LLM compile errors in typed code are type errors | TypeScript only, six open models of 2B–34B, undeclared names included | [[type-system]] |
| loops on non-compiling code dominate agentic cost | problem difficulty explains 80–97% of variance; the loops are a weak model's mechanism | [[token-cost]] |
| "docs plus compiler" shows the compiler is the most important half | documentation alone took Qwen3-Max from 7.74% to 46.45%; tools and docs work only together | [[training-prior]] |
| a spec in the prompt is not enough for very different languages (11.2%) | 11.2% needed interpreter feedback; agents on the same problems reached 86.9–99.7% | [[training-prior]] |
| Lean's 5× over OCaml comes from borrow inference plus reuse | turning borrow inference off made that benchmark faster; the gap is mostly OCaml's GC | [[memory-model]] |
| Perceus brought Koka within 10% of C++ | on the best case for reuse, which the authors say may favour counting | [[memory-model]] |
| GSM8K fell from 86.51 to 23.44 for Claude 3 Haiku in JSON mode | a schema in the prompt, not JSON mode; true constrained decoding lost 2.86 points | [[constrained-decoding]] |
| an incomplete grammar cuts correctness by up to 97% | one model, TOML, a grammar forbidding one optional space; other gaps cost nothing | [[constrained-decoding]] |
| type-constrained decoding is the step that reduces errors | a replication found compile rates up, functional correctness down in every configuration | [[constrained-decoding]] |
| a location-only rename passed 2 of 6 tasks | it passed about 4 of 6 | [[semantic-compiler]] |
| SWE-agent needed a guard against indentation errors | a general lint gate; indentation's share was never measured | [[editing-robustness]] |
| richer diagnostics help (24.6–63.4%) | one 14B model, two groups of programs, inversions per error; a stronger model scored 87.9–99.7% in every mode | [[semantic-compiler]] |
| SimPy saves 9–14%, the same order as variant A | counted with placeholder tokens added to every vocabulary; 13–15% on function-level code | [[token-cost]] |

The findings changed the design more than the corrections did: checking partial programs with the
compiler while the model writes, admissible alternatives as the content of a diagnostic, edits
addressed to syntax entities, and the cost of familiar syntax with a new meaning — see
[[semantic-compiler]], [[editing-robustness]] and [[lotml-syntax]].

The 25 papers added for [[compiler-embedded-model]] corrected the web survey that found them, which
had read abstracts, in the same way: CORE's 59.2% holds on a 520-file user-study subset, and its
25.8% fewer false positives is one of two figures the paper gives for the same filter; RTLFixer's
32.3% gain is 33.2 points by its own table; SLMFix's gains are in passing the validator, with
functional correctness unchanged; and AutoCommenter reached its 80% useful ratio by suppressing
practices, not by confidence thresholds.

## What it does not guarantee

- **A found quote proves the text exists, not that the claim reads it correctly.** The claim beside
  each quote is a reading; the wiki uses it only after checking it against the quote.
- **Conversion can garble** formulas and wide tables; numbers quoted from a table row are checked as
  the row's text.
- **Not every source is a paper.** Blogs, documentation, release notes and benchmark sites — Alderson,
  Dan Luu, aider's documentation, the Benchmarks Game, rustc's and OpenAI's documentation, MojoBench —
  are cited by link and not machine-checked; aider's 9× was re-read at its source in this pass.
- **Versions move.** A later arXiv version can change a number, as RustRepoTrans's did; the checksum
  in `sources.json` fails the fetch when the bytes change, so the claim is re-read instead of
  silently drifting.
