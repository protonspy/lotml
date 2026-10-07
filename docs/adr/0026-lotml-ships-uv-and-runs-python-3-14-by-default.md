---
status: accepted
---

# 0026 · lotml ships uv and runs Python 3.14 by default

## Context

`lotml run`, `lotml test`, `lotml bind` and the MCP `test` tool all need a CPython
(adr:0025-two-targets-python-for-run-llvm-for-build). Today `lotml-py` takes `LOTML_PYTHON`, or
else the first of `python3`, `python` and `py -3` that answers `--version`. When none is found,
it stops with "install Python 3.11 or later".

That leaves three problems:
- **The version is whatever the machine has.** The same program can run on 3.11 on one machine
  and 3.13 on another, and the parity suite's reference target moves with it.
- **`lotml bind` needs a separate install.** It needs mypy or jedi installed to find typeshed's
  stubs, or a stub passed by hand.
- **A missing Python is the user's problem.** It is the first thing a new user meets.

uv installs managed CPython builds (python-build-standalone), runs tools in ephemeral
environments, and is one static binary under MIT or Apache-2.0. It is already this repository's
tool for research and the harness (`docs/stack.md`). The owner chose to make it part of lotml's
package.

## Decision

**uv ships in the release archive.**
- The release archive of `lotml` carries a pinned uv binary next to `lotml`, with uv's licence
  texts and any third-party notices its release includes.
- The uv version and each target's SHA-256 are committed in the repository. The release workflow
  downloads that exact file over HTTPS from uv's release URL and fails on any mismatch; it never
  fetches the expected hash from the same place.
- A pin moves only by a pull request that changes the version and the hashes together.
- lotml never runs `uv self update`.

**lotml finds uv, in order, by absolute path only:**
- `LOTML_UV`, which must name an existing file by an absolute path;
- the uv beside lotml's own executable;
- `uv` on the path, searched explicitly, skipping relative and empty entries and never the
  current directory.

lotml logs which uv it used and that uv's version.

**lotml finds Python, in order, for `run`, `test`, `bind` and the MCP `test` tool:**
1. `LOTML_PYTHON`;
2. the project's virtual environment (`VIRTUAL_ENV`, then `.venv/` resolved inside the project
   root), for `run` and `test` only. Those run the project's code anyway, so its interpreter
   adds no new trust; `bind` never uses it;
3. a managed CPython 3.14 that uv already has;
4. downloading CPython 3.14 through uv. lotml first prints the version, the size and the host it
   downloads from, mirror included.

Python 3.14 becomes the default only after the parity suite passes on it. A build of lotml
without uv (a development build from cargo) falls back to today's search of the path.

**Every uv call is confined:**
- it runs from lotml's own cache directory, never from the project, with uv's configuration
  files ignored, so a project's `uv.toml`, `[tool.uv]` or `.python-version` cannot redirect a
  download;
- it forbids Python downloads except in the one install step, asks only for managed
  interpreters, and installs no shims on the user's path;
- it refuses to download while a variable that disables verification or replaces the download
  metadata is set. The variables lotml passes to uv are a documented list.

**`bind` gets typeshed without an installed mypy.** Without `--stub`, `lotml bind` takes
typeshed's stubs from a mypy wheel locked with hashes, binary only, from a fixed index, and reads
the stub files rather than running mypy. This replaces the search of an installed mypy or jedi
that adr:0012-python-interop-through-checked-boundaries-and-interface-files's tooling used. The
interface it writes is unchanged.

**The MCP server, the grader and the harness never download.** They are offline in code: an
environment variable can turn offline on but never off. They use an interpreter already
resolved, or they fail and name what is missing.

Rejected:
- Embedding uv as a Rust library: it is not a stable library API.
- Shipping a CPython inside the archive: tens of megabytes on every platform, and a second copy
  of what uv manages.
- Leaving discovery as it is: the three problems above.

## Consequences

- **The archive grows** by uv's binary, about 30 MB on each platform, and lotml's README says a
  third-party binary is bundled.
- **The first run downloads.** On a machine without Python 3.14, the first `lotml run` downloads
  it, about 30 MB, saying from where. An offline switch forbids any download.
- **lotml does not redistribute CPython.** uv fetches it at run time, so CPython's licences bind
  the download, not lotml's archive.
- **The cache is shared.** uv's cache is shared with the user's own uv, and a cache the user has
  poisoned is outside what lotml defends.
- **`lotml run` and `lotml test` record their interpreter and uv:** the interpreter's path and
  exact version, and uv's version. The parity suite and the harness then compare on a known
  version.
- **Native builds are unchanged.** `lotml build` still runs without Python (adr:0025).
- **Project dependencies are a later decision.** Declaring a project's Python dependencies so
  `lotml run` passes them to uv is not decided here.
