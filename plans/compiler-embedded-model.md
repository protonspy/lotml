---
autonomy: auto
ci: wait
---

# Compiler embedded model

Survey the prior work on a small language model specialised to one language and shipped inside its
compiler or toolchain — to locate and repair errors, explain diagnostics and suggest improvements —
and record it in the wiki with every number checked against its source.

## Why

Shipping a domain-specific small language model inside `lotml` raised the question of what has
already been tried. A first web survey found work in four areas — repairing syntax and compile
errors with small models, models specialised to one language, language models explaining compiler
errors, and learned lints — but its numbers came from abstracts and secondary pages. Done when the
open-access sources are in `research/literature/`, every number the wiki uses from them is pinned to
a verbatim quote that `verify.py` finds, and a wiki page says what the evidence implies for lotml
and what it leaves open.

## Paths

- `research/literature/`
- `docs/wiki/`
- `docs/glossary.md`

## References

- `docs/wiki/pages/semantic-compiler.md` — the diagnostics and repair loop such a model would extend
- `docs/wiki/pages/source-verification.md` — how sources and quotes are checked
- `plans/lotml-roadmap.md` — training or fine-tuning models is outside its scope

## Out of scope

- Training, fine-tuning or shipping a model: this records the evidence, and building one is the
  separate initiative the roadmap already sets aside.
- Machine-checking sources with no open PDF (ACM-only papers, blogs, product pages): they are cited
  by link, as the source-verification page already does for non-paper sources.

## Tasks

- [x] 1.1 (Unit) Add the surveyed open-access papers to `sources.json` with versioned URLs and fetch them
- [x] 1.2 (Unit) Read each new source in full and record its claims with verbatim quotes in `claims.json` until `verify.py` finds every quote
  _Depends 1.1_
- [x] 1.3 (Unit) Write the wiki page on a compiler-embedded model from the verified claims and link it from the index, the semantic compiler page and the changelog
  _Depends 1.2_
- [x] 1.4 (Unit) Update the source-verification counts and add the glossary term
  _Depends 1.3_

## Done when

- `scc validate` exits 0.
- `uv run --with pytest==9.1.1 pytest` in `research/literature/` passes and `uv run python verify.py`
  reports every quote found.
- The new wiki page is reachable from `docs/wiki/index.md`, and every number it takes from a paper in
  `sources.json` has a claim in `claims.json`.
