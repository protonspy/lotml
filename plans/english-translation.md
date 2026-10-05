---
autonomy: auto
ci: no-wait
---

# English translation

Translate every repository artifact written in Portuguese into English, so the project reads in
one language, and record English as the convention for future work.

## Why

The study landed in Portuguese, but the project's working language is English. Mixed languages
split the vocabulary: the glossary cannot hold one canonical term per concept when every concept
has two names. Done when docs/, plans/ and research/README.md are in English, wiki slugs and ADR
filenames are English, and `scc validate` is clean.

## Paths

- `docs/`
- `plans/`
- `research/README.md`

## Out of scope

- Commit messages and PR bodies already merged: history stays as written.
- The pilot specs, tasks and generated programs, which are already in English.
- Renaming the research records' provisional name X or the `.x` extension.

## Tasks

- [x] 1.1 (Unit) Translate the glossary and choose the English canonical terms
- [x] 1.2 (Unit) Translate the wiki pages, index and changelog, with English slugs
  _Depends 1.1_
- [x] 1.3 (Unit) Translate the ADRs under English filenames and update every citation
  _Depends 1.2_
- [x] 1.4 (Unit) Translate the notes, stack.md, research/README.md and the study plan
  _Depends 1.1_
- [x] 1.5 (Unit) Record English as the artifact language in the project conventions

## Done when

- No Portuguese prose remains under `docs/`, `plans/` or in `research/README.md`.
- `scc validate` exits 0.
- The pull request is open against `main`.
