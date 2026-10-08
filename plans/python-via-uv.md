---
autonomy: auto
ci: wait
pr: per-group
merge: manual
status: approved
checksum: 5d23a7dc85e28cfb4630c42abab0d4c56cd046cbd758eced2e9cfa07a8192b2a
---

# Python via uv

Ship a pinned uv beside the `lotml` binary, and let lotml provision and run CPython 3.14 through
it by default. `lotml run`, `lotml test` and `lotml bind` then work on a machine with no Python
installed, run on a known version, and cannot be redirected by a project's files.

## Why

Today lotml uses whatever `python3`, `python` or `py -3` the machine has, 3.11 or later. When
none is found it stops, and `lotml bind` needs mypy or jedi installed or a stub passed by hand.

adr:0026-lotml-ships-uv-and-runs-python-3-14-by-default decides the fix:
- uv ships pinned by version and hash;
- uv and Python are found by absolute path, in a fixed order;
- every uv call is confined away from the project's configuration;
- Python 3.14 becomes the default only after parity passes on it;
- the MCP server, the grader and the harness never download.

Done when a fresh machine with only the release archive runs `lotml run`, `lotml test` and
`lotml bind textwrap` on Windows and Linux, and the parity suite passes on 3.14. A project
directory's uv configuration must change nothing, and the docs must describe the new order.

## Paths

- `.github/workflows/release.yml`
- `.github/workflows/ci.yml`
- `compiler/crates/lotml-py/src/lib.rs`
- `compiler/crates/lotml-py/runtime/`
- `compiler/crates/lotml/src/exec.rs`
- `compiler/crates/lotml/src/mcp.rs`
- `compiler/crates/lotml/src/main.rs`
- `harness/lotml_harness/`
- `README.md`
- `reference/lotml.md`
- `docs/stack.md`
- `docs/wiki/pages/transpilation-strategy.md`

## References

- adr:0026-lotml-ships-uv-and-runs-python-3-14-by-default — the decision this builds
- adr:0012-python-interop-through-checked-boundaries-and-interface-files — what `bind` writes
- adr:0025-two-targets-python-for-run-llvm-for-build — native builds stay without Python
- `plans/bind-sources.md` — its venv task is folded into 1.3 here

## Out of scope

- Declaring a project's Python dependencies for `lotml run` to pass to uv: a later decision.
- Any change to `lotml build`, which runs without Python.

## Tasks

- [x] 1.1 (Unit) Run the parity suite and the Python-target tests on CPython 3.14 through `LOTML_PYTHON`, and fix what differs from 3.11–3.13, before anything makes 3.14 the default
- [x] 1.2 (Unit) Commit uv's version and each target's SHA-256. Have the release workflow download that exact file over HTTPS from uv's release URL, fail on a mismatch, and put it beside `lotml` with uv's licence texts and third-party notices, adding those files to `SHA256SUMS`. A script that lists the archive verifies this in CI
- [x] 1.3 (Unit) Find uv by absolute path: `LOTML_UV` (an existing file, absolute), then the uv beside the running executable, then an explicit search of the path that skips relative and empty entries and the current directory, logging the uv chosen and its version
- [x] 1.4 (Unit) Run every uv call from lotml's cache directory, with uv's configuration files ignored. Forbid Python downloads outside the install step, ask only for managed interpreters, install no shims, and refuse to download while a variable that disables verification or replaces download metadata is set. Each rule gets a test using a project with a hostile `uv.toml` and `.python-version`
  _Depends 1.3_
- [ ] 1.5 (Unit) Resolve the Python for `run`, `test`, `bind` and the MCP `test` tool in the order adr:0026 sets:
  - `LOTML_PYTHON`;
  - the project's venv for `run` and `test` only;
  - a managed 3.14 uv already has;
  - a download that first prints version, size and host;
  - today's path search when no uv is found.

  Update the "install Python 3.11 or later" message.
  _Depends 1.1, 1.4_
- [ ] 1.6 (Unit) Add the offline switch to `run`, `test` and `bind` (a flag and `LOTML_OFFLINE`, which can only turn offline on). Make the MCP server, the grader and the harness offline in code, so their resolver cannot reach the download step, with a test that they make no network request
  _Depends 1.5_
- [ ] 1.7 (Unit) Record in the JSON of `lotml run` and `lotml test`, and in every harness row that runs the Python target, the interpreter's path and exact version and uv's version
  _Depends 1.5_
- [ ] 2.1 (Unit) When `lotml bind` gets no `--stub`, take typeshed's stub from a mypy
      wheel locked with hashes, binary only, from a fixed index, by reading the stub
      files rather than running mypy. Validate the module name as a dotted identifier,
      honour the offline switch, and keep `--stub` as the override
  _Depends 1.4, 1.6_
  _Status removed_
  _Reason adr:0032 embeds typeshed's stdlib and reads stubs in Rust, so bind needs no mypy wheel_
- [ ] 3.1 (Unit) Have the harness provision Python 3.14 ahead of a run and always run offline, with a test that a row with no interpreter fails naming what is missing instead of downloading
  _Depends 1.6_
- [ ] 3.2 (Unit) Add CI jobs on Linux and Windows that start from the release archive on
      a runner with no Python on the path. They run `lotml run`, `lotml test` and `lotml
      bind textwrap`, then the same with the offline switch, and check that the run
      fails naming what is missing
  _Depends 1.2, 1.6_
- [ ] 3.3 (Unit) Describe uv, the resolution order, the offline switch and the recorded
      interpreter in `README.md`, `reference/lotml.md` and
      `docs/wiki/pages/transpilation-strategy.md`. Replace the CPython and typeshed
      entries of `docs/stack.md` with what the code now does
  _Depends 1.6, 1.7_

## Done when

- A release archive holds `lotml`, uv and uv's licences. On a runner with no Python, `lotml run` downloads 3.14 once, saying from where, and runs.
- With the offline switch and no Python, `lotml run` fails naming what is missing and downloads nothing.
- A project carrying a hostile `uv.toml` or `.python-version` changes neither the interpreter nor the download source.
- The parity suite passes on CPython 3.14.
- `cargo test`, clippy, `uv --directory harness run pytest` and ruff pass, and `scc validate` exits 0.
