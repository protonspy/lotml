---
autonomy: auto
ci: wait
status: approved
checksum: 96c8314b4d2f09b9d667e67bce38e83eb47bad8adba40613c4422c289b2b12d6
---

# Merged guide specs

Record in each spec's frontmatter that the pull request delivering it is merged.

## Why

`scc spec sync` reads git and the forge back into the specs, and five still say `in-review`
although their pull requests are merged: `agent-humaneval`, `seeded-failures` and `trace-dataset`
by #18, `guide-records` and `guide-tool` by #20. A spec whose state is stale reads as unfinished
work. Done when no spec is behind what git and the forge say.

## Paths

- `specs/agent-humaneval/requirements.md`
- `specs/seeded-failures/requirements.md`
- `specs/trace-dataset/requirements.md`
- `specs/guide-records/requirements.md`
- `specs/guide-tool/requirements.md`

## Tasks

- [x] 1.1 (Unit) Write the merged state of each spec back with `scc spec sync`
- [x] 1.2 (Unit) Give the guide's notes n-0056 to n-0059, which #16 also used, the next
      free ids
  _Reason the commit hook refused every commit: main holds four duplicate note ids_

## Done when

- `scc spec sync --dry-run` reports no change.
- `scc validate --checks --pr` exits 0.
