---
autonomy: auto
ci: wait
status: approved
pr: per-group
merge: manual
checksum: a8ca7f36ab7bb753abf04951ff1b12af3f2821d23c2b31b349d08cbe36be69c6
---

# IR architecture

Restructure the compiler so that one IR sits between the checker and every backend, then grow
LotML's native side on it: an LLVM backend, `lotml build` making executables, native programs
calling CPython, and LotML functions exported through the C ABI.

## Why

Each backend reads the checked program its own way today — the Python backend from the syntax
tree, the C backend from a form private to it — so every rule past the checker is lowered twice
and held together only by the parity suite (adr:0020-one-ir-between-the-checker-and-every-backend).
The native backend the roadmap left for phase 4 would be a third reading. This plan makes the
pipeline syntax, checker, IR, backends; puts LLVM on it as the native code generator
(adr:0021-compiler-in-rust-with-llvm-as-its-native-code-generator) past a phase 3 gate recorded as
failed on numeric performance (adr:0019-proceed-to-phase-4-past-the-failed-phase-3-gate); and
closes the gaps of native code against the Python world: no Python import, and no way for C to
call LotML. Done when every target reads the IR and passes the same suite, and `lotml build`
makes an executable.

## Paths

- `compiler/crates/lotml-ir/` — the IR, its lowering and its passes (new)
- `compiler/crates/lotml-c/` — the C backend, re-based on the IR, then retired once LLVM passes parity
- `compiler/crates/lotml-py/` — the Python backend, re-based on the IR
- `compiler/crates/lotml-llvm/` — the LLVM backend (new)
- `compiler/crates/lotml/src/exec.rs` — how `build`, `run` and `test` drive each target
- `harness/lotml_harness/experiments/` — the parity and benchmark runs

## References

- `specs/shared-ir/` — `lotml-ir`: the lowering and the native passes moved out of the C backend
- `specs/python-on-ir/` — the Python backend reading the IR instead of the syntax tree
- `specs/llvm-backend/` — `--target llvm` for numbers, `bool`, control flow, calls and printing
- `specs/llvm-parity/` — the LLVM target compiling everything the C target does
- `specs/python-bridge/` — native programs importing Python modules through an embedded CPython
- `specs/c-abi-export/` — a shared library and its C header for the functions a program exports
- `specs/c-backend/` — the C target, its runtime and its parity contract, folded into llvm-parity
- `plans/lotml-roadmap.md` — the phases this re-scopes from task 5.1 on
- adr:0019-proceed-to-phase-4-past-the-failed-phase-3-gate
- adr:0020-one-ir-between-the-checker-and-every-backend
- adr:0021-compiler-in-rust-with-llvm-as-its-native-code-generator
- adr:0022-lotml-build-makes-a-native-executable-by-default
- adr:0016-c-target-as-monomorphic-c-over-a-counting-runtime
- adr:0012-python-interop-through-checked-boundaries-and-interface-files
- adr:0013-c-libraries-through-interfaces-named-c
- adr:0023-native-programs-load-cpython-at-run-time-through-its-stable-abi
- adr:0024-c-abi-exports-chosen-by-signature-without-new-syntax

## Out of scope

- Syntax: `struct`, `enum`, `null`, `class`, `try`, `with` and `yield` stay as
  adr:0002-errors-as-values and adr:0010-variant-b-and-indented-blocks-settled-by-the-phase-0-gate
  settled them.
- Cranelift, dropped by adr:0021-compiler-in-rust-with-llvm-as-its-native-code-generator.
- A C fallback when `clang` is missing, or the C target kept behind a flag of its own
  (adr:0021-compiler-in-rust-with-llvm-as-its-native-code-generator).
- Closing phase 3's numeric gap: measured again on the LLVM target here, fixed separately.
- Another memory model: counting with reuse stays
  (adr:0008-value-semantics-with-reuse-before-borrowing).

## Tasks

- [x] 1.1 (Unit) Move the C runtime out of `lotml-c` into a crate both native backends write it
  from, the parity run unchanged
- [x] 1.2 (Unit) Write and build the spec shared-ir, the C target's output, parity and benchmarks
  unchanged
  _Depends 1.1_
- [ ] 1.3 (Unit) Write and build the spec python-on-ir, the parity suite unchanged
  _Depends 1.2_
  _Status removed_
  _Reason reorder after analysing the research report: the LLVM target needs only the monomorphic IR, so the first executable lands before the Python backend's rewrite; re-added as 3.3_
- [x] 2.1 (Unit) Write and build the spec llvm-backend: `add`, `fib`, `collatz` and `mandelbrot`
  print on `--target llvm` what they print on the Python target
  _Depends 1.2_
- [ ] 2.2 (Unit) Write and build the spec llvm-parity: the parity suite identical on the
      Python,
  C and LLVM targets, and the benchmarks run on LLVM recorded beside the C run
  _Depends 2.1_
  _Status removed_
  _Reason re-added as 2.6 with two targets (adr:0025-two-targets-python-for-run-llvm-for-build)_
- [x] 2.3 (Unit) Make `--target llvm` the default of `lotml build` (adr:0022),
      correcting what
  describes `build` as writing Python
  _Depends 2.6_
- [ ] 3.1 (Unit) Write and build the spec python-bridge, lifting the native refusal of
      Python
  imports on the LLVM target
  _Depends 2.5_
  _Status removed_
  _Reason dropped: native programs do not load CPython; a program importing Python runs under lotml run (adr:0025-two-targets-python-for-run-llvm-for-build)_
- [x] 3.2 (Unit) Write and build the spec c-abi-export: a C program calling a function from a
  library `lotml build` wrote
  _Depends 2.5_
- [x] 4.1 (Unit) Narrate the pipeline in `docs/codewiki/` and bring the README's
      architecture and
  status, `docs/stack.md` and the wiki's transpilation strategy up to date
  _Depends 2.3, 3.2, 3.3_
- [x] 2.4 (Unit) Pass `--target python` where the harness experiments call `lotml build`
      and import the module it wrote (`gate2.py`, `phase1.py`), so moving the default
      does not change what they run
  _Depends 2.6_
  _Reason review of the plan: adr:0022's context missed these two callers of build_
- [x] 2.5 (Unit) Retire the C target: remove `--target c`, the C emitter and its
      compiler
      discovery, keep the runtime crate `clang` compiles, run the parity suite on the Python and
      LLVM targets, and fold specs/c-backend into specs/llvm-parity
  _Depends 2.1_
  _Priority 1_
  _Reason the user chose LLVM as the only native target (adr:0021)_
- [x] 3.3 (Unit) Write and build the spec python-on-ir: one lowering from the syntax
      tree to the IR, read with generics intact by the Python backend and after
      monomorphization by the native ones, the parity suite unchanged
  _Depends 1.2_
  _Reason reorder after analysing the research report: the LLVM target needs only the monomorphic IR, so the first executable lands before the Python backend's rewrite; replaces 1.3_
- [x] 2.6 (Unit) Write and build the spec llvm-parity: the parity suite identical on the
      Python and LLVM targets, and the benchmarks run on LLVM recorded beside the C
      target's last run
  _Depends 2.5_
  _Reason replaces 2.2, which named the C target that adr:0025-two-targets-python-for-run-llvm-for-build retires first_

## Done when

- No backend crate imports `lotml_syntax::ast`; each reads `lotml-ir`.
- `harness/results/parity.md` shows the same rows on the Python and LLVM targets, and
  `--target c` is gone.
- `lotml build add.lot` with no flag writes an executable through `clang`.
- A natively built program calls a Python module, and a C program calls a LotML function.
- `scc validate` exits 0.
