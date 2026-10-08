# Target parity

lotml has two targets (adr:0025-two-targets-python-for-run-llvm-for-build): `lotml run` compiles
to Python and `lotml build` to native code through LLVM. The Python target is the reference, and
the language promises that a program means the same on both. adr:0020-one-ir-between-the-checker-and-every-backend
makes that likely by construction, because both backends read one IR. Testing is what shows it
holds. This page gathers what the projects in [[prior-art-compilers]] learned about proving it,
and what lotml's parity suite still lacks.

## What lotml has

- **Rust tests.** `compiler/crates/lotml-llvm/tests/` runs hand-written programs on the LLVM
  target at `-O0` and `-O2` and compares them with the Python target (`common/mod.rs`,
  `parity`). `frees_everything` checks that no counted cell leaks.
- **The corpus suite.** `harness/lotml_harness/experiments/parity.py` runs the `test` blocks of
  every corpus program on both targets. It classes each program as `same`, `differs`, `refused`
  (a Python import), `not compiled` or `no report`, and writes `results/parity-llvm.md`.
  All 509 programs report `same` today. The overall result is all-or-nothing, though, and the
  suite runs by hand: nothing in CI holds it.

That is stronger than most of the prior art:

- **LPython** only asserts and checks exit codes; output is never compared with CPython, so
  float printing drifted unnoticed.
- **plix's** Windows CI runs only `--version`.
- **depyler's** "semantic equivalence" test compares against a stand-in written in Rust instead
  of Python, with truncating `//` and saturating `+`. Its own parity report shows 0 matches out of
  30.

## A floor that only rises

pon keeps one committed file per suite: the modules that pass, plus a minimum count
(`pon-conformance/src/ratchet.rs`). CI fails only when a module that passed stops passing, or
the count drops. It does not fail because some modules have never passed. Each result is classed
pass, fail, refused or error, and refusals are grouped by message into a committed JSON.
Updating the floor is refused on a filtered run. Built executables run with the environment
cleared.

The point is what an all-or-nothing flag cannot do. While a backend is closing a gap, the flag
is red, says nothing about regressions, and so cannot be a gate. A floor can be a gate from the
first day. With all 509 programs passing, lotml's floor today would be the whole corpus. What
the floor adds is a CI gate, and a way to admit new programs — fuzzer finds, new language rules
— before the native target handles them.

lotml's floor is built (plans/target-parity-assurance.md): `harness/results/parity/floor.json`
holds the 509 programs, and CI runs `parity floor check --since <base>`. A run fails when a program
of the floor stops reporting the same or the count drops; a program the base's floor held and this
one does not must carry a reason, which `floor update --reason` records. The update refuses a run
filtered by `--only`. Refused and uncompiled programs are grouped by message in
`results/parity/unsupported.json`, and the native builds run with only what finds `clang` and the
runtime's cache in their environment.
**LPython shows the opposite failure:** after one change, 188 of its 400 integration tests were
commented out of a CMake file, and since nothing counted them, nothing noticed. A gap that is not
machine-readable grows silently.

## Fuzzing one target against the other

Hand-written programs test what their authors thought of. The prior art fuzzes:

- **pon:** a template fuzzer that marks chunks of a program. It compares the exception class
  rather than stderr bytes, and minimizes a failing case by deleting chunks greedily (`fuzz.rs`).
- **SPy:** a hypothesis-based test that runs two pipelines on generated inputs.
- **plix:** a seeded generator of type-checked programs, run both ways and diffed.

For lotml, the oracle is the Python target. Typed programs come from templates and from
`lotml dev mutate`. One adaptation matters: a panic ends a native process, so many cases go into
one `main` per program rather than one call each. Native builds also run with cell counting and
`LOTML_SANITIZE`, which catch reference-counting bugs CPython cannot show.

That fuzzer is built: `python -m lotml_harness.experiments.differential` runs templated programs,
forty cases each, and corpus mutants on both targets, compares output, exit and the kind of a
panic, continues a batch past a panic both share, minimizes a difference by deleting chunks and
writes it to `harness/results/parity/fuzz/`. `LOTML_COUNT_CELLS` asks the native build to report
its live cells. Its first runs found two real differences, both in the Python target's runtime:
`(-1.5) ** 0.1` gave CPython's complex number, where the native target raises `ValueError`; and
`sum` of integers checked only its result, so `sum([3, 9223372036854775807, -7])` printed where
the same additions written out trap, as they do natively. Both now behave as natively
(plans/target-parity-assurance.md 2.4, 2.5). A panic is compared by its kind: the native target's
`lotml test` trace names only the panicking frame (n-0101), and the Python target's output on a
piped Windows console is in the ANSI code page (n-0100), so the fuzzer runs it in UTF-8.

The same fuzzer pointed only at the frontend — arbitrary text into the parser, the checker and
the lowering — looks for panics. Those matter because the language and MCP servers feed the
compiler whatever an editor or agent sends. That one is built: `compiler/fuzz/`, cargo-fuzz over
libFuzzer on nightly (adr:0028-fuzz-the-frontend-with-cargo-fuzz-on-nightly), seeded with the
corpus. Its first ten-minute run, 69,091 inputs, found no panic; the token soup and line-cut tests
of `lotml-syntax` had already found and fixed a string escape whose span ran past the text.

## Every test in every mode

SPy writes each test once and runs it as interp, doppler and C, with skips named per backend.
That is the shape of a conformance matrix. pycc derives one mechanically: each fixture declares
what it proves and what it leaves out, as a core gap or as out of scope, and each row's status is
computed from those declarations, not written by hand. For lotml, each row is a language rule —
overflow, `/` on integers, `match`, errors as values — and its status on each target.

lotml's is built: `compiler/crates/lotml-llvm/tests/conformance.json` declares the rules and, for
each of the 71 parity programs, the rules it proves and the ones it leaves out with why;
`python -m lotml_harness.experiments.conformance` checks the manifest against the programs the tests
run and writes `harness/results/parity/conformance.md`. It shows 26 of 28 rules proved, and no
parity program yet proving optionals or `test` blocks.

## Rules both targets must enforce the same way

Some behavior lives outside the IR and differs today:

- **Recursion depth.** The Python target stops at CPython's 1,000 frames. A native program stops
  when the operating system's stack runs out, which is 1 MiB on Windows. Neither side sets a
  limit. Monty counts depth per call and charges its limits every 255 steps at about 2% cost.
  Edge-python charges an operation budget only on taken back-edges, calls and builtins that cost
  in proportion to size. `specs/recursion-depth/` specifies one limit for both: 1,000 calls of the
  functions that can recurse, counted by the program itself on each thread, a `RecursionError`
  past it, and native threads reserving the stack the limit needs.
- **Windows.** CI tested on Ubuntu, and the Windows release job only runs `--version`. Plix
  shipped native code with the Unix calling convention against a runtime using the Windows one,
  because its Windows CI ran only `--version`. CI now runs the LLVM target's tests on a Windows
  runner too, and the tests read what a program imports and a library exports from the bytes,
  with their own PE and ELF reader: a native program loads no libpython.

The work is in `plans/target-parity-assurance.md`.
