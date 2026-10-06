---
autonomy: auto
ci: wait
status: approved
checksum: b2f5cd2f0ba97083ffa903d2eb87607d1a5eaca9daf376703c9b4d7d18e6e3a0
---

# Merged specs

Record in each spec's frontmatter that the pull request delivering it is merged.

## Why

`scc spec sync` reads git and the forge back into the specs, and three still say `in-review`
although their pull requests are merged: `agent-guide` and `agent-harness` by #11, `c-backend` by
#16. A spec whose state is stale reads as unfinished work. Done when no spec is behind what git
and the forge say.

## Paths

- `specs/agent-guide/requirements.md`
- `specs/agent-harness/requirements.md`
- `specs/c-backend/requirements.md`

## Tasks

- [x] 1.1 (Unit) Write the merged state of each spec back with `scc spec sync`

## Done when

- `scc spec sync --dry-run` reports no change.
- `scc validate --checks --pr` exits 0.
