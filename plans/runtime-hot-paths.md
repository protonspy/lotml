---
autonomy: auto
ci: wait
pr: per-group
merge: manual
status: approved
checksum: fc5303fa5ecff176350ae9df613c1ff76d2f07c32deaa14d6b99418a67e268d7
---

# Runtime hot paths

Take the per-operation costs out of native programs and of `lotml run`, each change held to the
benchmarks before and after and to the parity suite. The native costs are out-of-line reference
counting, allocated constant literals, linear string indexing and looped float printing. On the
Python target they are module loading and interpreter start-up.

## Why

Native lotml runs 1.74x–5.03x the time of hand-written C on four of five benchmarks
(`harness/results/benchmarks-llvm.md`). The study of the prior art found avoidable costs on paths
every program takes:

- `lt_inc` is an out-of-line call (`see n-0094`);
- every list literal allocates and pushes element by element;
- `s[i]` is O(n) on non-ASCII text (`see n-0092`);
- `lt_shortest` loops through `snprintf` (`see n-0093`).

On the Python target, every load rebuilds the module through JSON, `ast` and `compile()`, and
every run starts a fresh CPython.

Done when each change below has landed with its benchmark recorded, or been dropped for a
benchmark that did not move. See `docs/wiki/pages/compiler-performance.md`.

## Paths

- `compiler/crates/lotml-llvm/src/emit.rs`
- `compiler/crates/lotml-runtime/c/`
- `compiler/crates/lotml-py/runtime/lotml_rt.py`
- `harness/lotml_harness/experiments/benchmarks.py`
- `harness/benchmarks/`
- `compiler/crates/lotml-runtime/c/THIRD_PARTY_NOTICES.md`

## References

- adr:0003-value-semantics-with-reference-counting
- adr:0008-value-semantics-with-reuse-before-borrowing
- adr:0016-c-target-as-monomorphic-c-over-a-counting-runtime
- adr:0025-two-targets-python-for-run-llvm-for-build
- `research/prior-art/studies/spy.md` (Ryu), `RustPython.md` (code-point index, Timsort), `starlark-rust.md` (constant literals, small dicts), `monty.md` (worker pool), `erg.md` (code object cache)

## Out of scope

- A tracing collector or borrowed parameters without a benchmark: adr:0003 and adr:0008.
- Copying code from pon, interpreter-rs or tarvos, whose licenses forbid it.

## Tasks

- [ ] 1.1 (Unit) Add benchmarks for counting-heavy code, constant list and dict literals, indexing non-ASCII strings, printing floats, sorting and small dicts, and record the LLVM target's baseline
- [ ] 1.2 (Unit) Emit the fast path of `lt_inc` and `lt_dec` inline in the LLVM IR, calling the runtime only to free
  _Depends 1.1_
- [ ] 1.3 (Unit) Emit an all-constant list or dict literal as a static cell with a count of 0, copied before its first write as string literals already are
  _Depends 1.1_
- [ ] 1.4 (TDD) Index strings by code point in O(1) on non-ASCII text with a sparse code-point index kept beside the bytes, after RustPython's `Wtf8Index`, with its MIT notice in the runtime's third-party notices
  _Depends 1.1_
- [ ] 1.5 (TDD) Print floats through Ryu with a normalizer to CPython's `repr`, after SPy's `libspy/src/str.c`, with Ryu's and SPy's notices in the runtime's third-party notices
  _Depends 1.1_
- [ ] 1.6 (TDD) Sort lists with Timsort, ported from CPython's `listsort` (`Objects/listobject.c`, PSF-2.0), which RustPython's `sorting.rs` follows, with the PSF notice in the runtime's third-party notices; stable as the merge sort is
  _Depends 1.1_
- [ ] 1.7 (Unit) Scan dicts of up to 16 entries without building a hash index, keeping insertion order, if the benchmark of 1.1 moves
  _Depends 1.1_
- [ ] 2.1 (Unit) Cache the compiled code object of a module in `lotml_rt.py`, in process memory only and never read from disk, keyed by its payload, instead of rebuilding it through `ast` and `compile()` on every load
- [ ] 2.2 (Unit) Specify, as a new spec named python-worker-pool, a pool of started CPython workers behind `lotml run`, `lotml test`, the MCP `test` tool and the grader, with its limits and its isolation: each program gets a fresh interpreter state, working directory and environment, a deadline and a memory cap, and a crash replaces the worker (adr:0025)
- [ ] 1.8 (TDD) Compare the `repr` of rounding-tie doubles, RustPython's `float.rs`
      cases among them, with CPython's before 1.5's Ryu printing is merged
  _Reason review of the literature on 2026-10-08: research/prior-art/studies/RustPython.md warns a Ryu-style replacement inherits the tie problem, and the branch's commit c992098 has no tie test_
- [ ] 1.9 (Unit) Count the reference-count operations each benchmark runs, and if they
      dominate, specify borrow inference for read-only parameters as a shared-ir delta,
      held to reuse not regressing (adr:0008)
  _Reason review of the literature on 2026-10-08: the IR report recommends removing Inc and Dec in IR passes, 1.2 only makes them cheaper, and counting.lotml now measures them_

## Done when

- `harness/results/benchmarks-llvm.md` shows each benchmark before and after its task.
- The parity suite still reports every program `same`, and `frees_everything` passes at both levels.
- Every ported file's provenance is checked, and `THIRD_PARTY_NOTICES.md` carries its license.
- `cargo test`, clippy, `uv --directory harness run pytest` and ruff pass.
- The python-worker-pool spec exists, and `scc validate` exits 0.
