# Python dependencies — design

## What changes

Serves R1.1, R1.2, R1.3, R1.4, R2.1, R2.2, R2.3, R3.1, R3.2.

A new module, `compiler/crates/lotml/src/dependencies.rs`, sits between the interpreter
`exec::python` resolves and the program it runs.

- **The project** is `exec::project_of`'s: the nearest directory holding `.git` or a
  `pyproject.toml`. A root that is a drive's root, or on Unix a directory others may write or whose
  `uv.lock` another user owns, is ignored with a line saying so (R2.3), as `resolve::environment`
  ignores such a `.venv`. When the root holds a `uv.lock`, the lock is read — a regular file, at
  most 64 MiB — and checked (R2) before anything runs.
- **The base interpreter** is resolved as adr:0026 orders it, without the project's `.venv`:
  `LOTML_PYTHON`, a managed CPython 3.14, a download where one is allowed, the path. `LOTML_PYTHON`
  still chooses the interpreter, the lock chooses the packages, and the lock's environment stands
  where the `.venv` would have. `resolve::Use::Bind`, which `lotml bind` no longer needs, becomes
  `Use::Base`, the use that never takes the project's environment.
- **The key** is the SHA-256 of the lock, the `pyproject.toml` and the base interpreter's path and
  full version, as the interpreter reports them, through `lotml_llvm::sha256`, made public. The environment lives in
  `<lotml's cache>/python-environments/<key>/`, a directory private to the user
  (`cache::private_directory`), next to a work directory holding the two copied files.
- **Making it** (R1.2) is one `uv sync` through `Uv::command`, so it runs from lotml's own
  directory with uv's configuration ignored (adr:0026): `--frozen --no-build --no-install-project
  --python <base> --project <work>`, with `UV_PROJECT_ENVIRONMENT=<environment>`. It is made in a
  directory of its own, `lotml-complete` written only after uv succeeds, and renamed into place
  whole; a directory without the mark is made again, so neither a stopped run nor two runs at once
  leave a half environment in use (R1.3).
- **Where nothing installs** (R3): the `downloads` flag `run` and `test` already carry, false for
  `--offline`, `LOTML_OFFLINE`, the MCP server, the grader and the harness, also forbids the sync.

## Data

`uv.lock` is TOML, read with the `toml` crate into a table. Each `[[package]]` has a `source`
table of one key:

| `source` | Accepted when |
|---|---|
| `registry` | it is `https://pypi.org/simple` |
| `virtual`, `editable` | it is `"."`: the project's own entry, never installed |
| `git`, `url`, `path`, `directory`, anything else | never |

Each `sdist` and each entry of `wheels` must have a `url` under `https://files.pythonhosted.org/`
and a `hash`. A lock that does not parse, has no `[[package]]` list, or breaks one of these rules
is refused with the package's name and the rule (R2).

## Alternatives considered

- **`uv sync --locked`**, as adr:0033 words it: when the manifest and the lock disagree, uv
  resolves the manifest again before it refuses, and fetches every URL the manifest names to do so —
  addresses the project chooses, reached before lotml's check of the lock means anything. lotml has
  already checked the lock it installs, so `--frozen` installs that lock as it stands and resolves
  nothing; uv refuses `--no-sources` beside it, having no sources to read.

- **Reading the lock line by line**: uv writes it in a fixed layout, but the lock is the project's
  input, and a parser that a reformatted file walks around would refuse nothing. `toml` is the
  parser Cargo uses; it is a new dependency, recorded in `docs/stack.md` under adr:0033.
- **`uv export` to read the sources**: it would put uv's interpretation of the lock between lotml
  and the check, and run uv on a lock not yet checked.

## Risks

- The sync downloads wheels on the first run of a lock. That run needs the network; lotml says it
  is installing before uv starts, and uv's own progress stays off, as adr:0026 runs it.
