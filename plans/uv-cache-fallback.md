---
autonomy: auto
ci: wait
status: approved
checksum: 5be58e8561f50bb08291e8e5630abd2289d033d1899570ea34b0c3bbbdcdf103
---

# Uv cache fallback

When lotml finds a uv but refuses the cache directory uv would run from, it runs no uv and quietly
falls back to `python3`, `python` or `py -3`. Make that an error that says why, after
`LOTML_PYTHON` and the project's virtual environment, as adr:0026 orders.

## Why

adr:0026-lotml-ships-uv-and-runs-python-3-14-by-default orders the interpreter as `LOTML_PYTHON`,
the project's virtual environment, the CPython 3.14 uv installed, a download through uv, and the
path's interpreters only when there is no uv. `exec::python` builds the confined uv only when the
cache directory passes `private_directory`; when it does not, `resolve` is told there is no uv and
runs whatever the path names. On Windows a `LOTML_CACHE_DIR` outside the user's profile is refused,
and `py.exe` in `C:\Windows` is found even with an empty `PATH`. The release-archive CI job ran the
runner's own Python this way, with nothing said (n-0125). Done when a found uv with an unusable
cache is an error naming the uv, the directory and how to proceed, and every earlier step of the
order still wins over it.

## Paths

- `compiler/crates/lotml-py/src/resolve.rs`
- `compiler/crates/lotml/src/exec.rs`

## References

- adr:0026-lotml-ships-uv-and-runs-python-3-14-by-default — the order this restores

## Out of scope

- What makes a cache directory usable: `lotml_llvm::cache` decides, and is not changed.

## Tasks

- [x] 1.1 (Unit) Make a found uv whose cache lotml refuses an error naming the uv and the directory, reached only after `LOTML_PYTHON` and the virtual environment, never the path's interpreters

## Done when

- A test of `resolve` shows `LOTML_PYTHON` and the virtual environment still win, and that a found uv with an unusable cache is an error rather than the path's interpreter.
- `cargo test`, clippy and `scc validate` pass.
