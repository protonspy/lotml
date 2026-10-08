# Rust binder — design

## What changes

Serves R1.1, R1.2, R1.3, R1.4, R2.1, R2.2, R2.3, R2.4, R3.1.

A new crate, `compiler/crates/lotml-bind`, holds the binder and the stubs lotml carries; it
depends on `ruff_python_parser` and `ruff_python_ast`, pinned to an exact version, and on nothing
else of lotml's. It is its own crate so the parser's compile time stays off the crates that do not
read Python, and so the spec bind-on-import can call it from `lotml check` as `lotml bind` does
(adr:0032-a-python-module-is-bound-at-check-time-from-stubs-read-in-rust).

- **`binder`** turns a stub's text into an interface's text. It ports `lotml_bind.py` rule for rule:
  the type map, the defaults, `if sys.version_info …` blocks walked with the first definition
  winning, the aliases typeshed writes as `name = _inst.method`, overloads and coroutines listed as
  not bound, and `PyObject` for what no LotML type describes (adr:0031). One function, `escape`,
  writes every string the stub gives the interface: `\\`, `"` and `\n` as before, and every other
  control character, U+2028 and U+2029 as `\xNN` or `\uNNNN`, which LotML's strings read back
  (R1.3).
- **`typeshed`** is the embedded `stdlib`: looked up by module name, gated by `VERSIONS` for
  CPython 3.14, the version adr:0026 runs by default (R2.3). The set of top-level names in `VERSIONS`
  is what "a standard-library name" means for R2.4, replacing the Python binder's
  `sys.stdlib_module_names`.
- **`lotml bind`** (`compiler/crates/lotml/src/exec.rs`) calls the crate in-process: a given stub,
  else typeshed's, else, for a name that is not the standard library's, the project's packages as
  `lotml_py::sources` finds them (plans/bind-sources.md 1.1). It resolves no interpreter, so its
  `--offline` flag, added for a download it can no longer make, goes. `lotml_bind.py` and
  `lotml_py::BIND` go once the parity of R1.2 is shown.
- **The binding coverage report** (`harness/lotml_harness/experiments/binding_coverage.py`)
  binds each module of its corpus with `lotml bind <module> --stub <pinned stub>` and counts the
  `fn` lines of the interface written, instead of loading `lotml_bind.py` as a module (R3.1).

## Data

The vendored stubs live at `compiler/crates/lotml-bind/typeshed/`: `stdlib/` as typeshed has it,
`LICENSE`, and `COMMIT`, the full SHA of the typeshed commit they were copied from. They are plain
text so that moving the pin is a pull request whose diff shows which stubs changed, which the
owner chose on 2026-10-08. `release/typeshed.py <commit>` replaces them with that commit's.

At build time `build.rs` packs `stdlib/` into one blob — each file's path and text, in path order —
and deflates it with `miniz_oxide`; the binary holds the compressed blob, about 0.5 MB for 4.6 MB
of stubs, and inflates it once, on the first lookup. A stub read from the blob is the vendored
file byte for byte.

## Boundaries and contracts

A stub is hostile input (adr:0032). Before it is parsed, a stub larger than 8 MiB is refused, and
its tokens are scanned once: a bracket nesting deeper than 100, or a logical line of more than
20 000 tokens, is refused, which bounds the depth of the tree the parser builds and the binder
walks. The parser and the walk run on a thread with a 64 MiB stack, so a tree within those limits
cannot overflow it. The parse is linear in the stub's size, so the size limit bounds its time.

`lotml bind`'s exit status and messages keep their meaning: 0 bound, 2 refused, the message naming
the module and why.

## Alternatives considered

- **Embedding the stubs uncompressed**: no new dependency, but 4.6 MB more binary for text that
  deflates nine to one. `miniz_oxide` is pure Rust, already the deflate of Rust's own toolchain,
  and is a build dependency and a small runtime one.
- **Keeping the Python binder as the oracle in the test suite**: it would keep Python in the
  binder's tests forever. The parity of R1.2 is shown once, over every module of the vendored
  `stdlib` and every PyPI stub of the coverage corpus, in the commit that switches the binder; from
  then on goldens of the corpus's standard-library interfaces hold it, and moving the pin shows
  their diff.

## Risks

- `ruff_python_parser` is published as an internal crate of Ruff, with no promise of a stable API;
  it is pinned exactly, and a version bump is a change of its own.
- Python's `repr` of a float default is reproduced by hand — digits from Rust's shortest
  round-trip formatting, Python's switch to exponent notation below 1e-4 and from 1e16 — and the
  parity run is what shows it matches.
