---
autonomy: auto
ci: wait
pr: per-group
merge: manual
status: approved
checksum: 573349f098c21c955478586be322e22818d6dfe30437c21da1d25271e2fe7b8d
---

# Frontend robustness

Keep the parser, the checker and the lowering whole on any text an editor or agent sends:
recover from an unclosed bracket without losing the rest of the file, never panic, never loop,
and never let one node's type overwrite another's.

## Why

The language and MCP servers feed the frontend half-written code, and symbol-addressed edits
depend on the outline surviving it. Reading `lotml-syntax` against ruff's parser found two risks:

- an unclosed bracket drops every later definition (`see n-0089`);
- the checker keys expression types by span (`see n-0090`), which mamba paid for once.

Nothing fuzzes the frontend today. Done when each of those is fixed or shown not to happen, the
parser is guarded against looping, and a fuzzer has run against parse, check and lowering
without a panic. See `docs/wiki/pages/prior-art-compilers.md` and `research/prior-art/studies/ruff.md`.

## Paths

- `compiler/crates/lotml-syntax/src/`
- `compiler/crates/lotml-check/src/`
- `compiler/crates/lotml-ir/src/lower.rs`
- `compiler/crates/lotml-diag/src/codes.rs`
- `docs/stack.md`

## References

- adr:0009-significant-indentation-with-symbol-addressed-edits
- adr:0021-compiler-in-rust-with-llvm-as-its-native-code-generator — the hand-written parser stays
- `docs/wiki/pages/semantic-compiler.md`

## Out of scope

- Replacing `lotml-syntax` with ruff's parser: rejected in `docs/wiki/pages/prior-art-compilers.md`.
- Changing the language's syntax.

## Tasks

- [x] 1.1 (TDD) Reproduce, in a test, an unclosed `(` hiding every later `fn` from the outline, then recover at the next line that starts a definition at column 0 so the rest of the file parses
- [x] 1.2 (Unit) Guard every parser loop so it always consumes a token or stops, and test that each node's span is ordered and nested inside its parent's over the corpus
- [x] 1.3 (Unit) Lower an `elif` chain iteratively in the `if_chain` of `lower.rs`, or bound its length with a diagnostic, so a long chain cannot recurse through the compiler's stack
- [x] 2.1 (Unit) Check whether any desugared node reuses a source span in the checker's type map; if one does, key expression types by node identity and add the case as a test
- [x] 2.2 (Unit) Reject an invalid f-string format spec at check time with a diagnostic code, following Python's format-spec mini-language as RustPython's `FormatSpec` parser reads it
- [x] 3.1 (Unit) Choose a fuzzing tool for the compiler, recording it in `docs/stack.md` and, as a new dependency, in an ADR, and fuzz text into parse, check and lowering until no input panics
  _Depends 1.1, 1.2_

## Done when

- `cargo test --manifest-path compiler/Cargo.toml` passes, with tests for 1.1, 1.2, 1.3 and 2.2.
- The fuzzer runs for ten minutes from a documented command without a panic.
- `cargo clippy --manifest-path compiler/Cargo.toml --locked --all-targets -- -D warnings` passes.
