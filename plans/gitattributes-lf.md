---
autonomy: auto
ci: wait
status: approved
checksum: 807bde5c9242e122e454347143a47056025cc2b1ec22198a2d2c89db860de185
---

# Line endings

Check out every text file with LF on every platform, as the repository already stores it.

## Why

A Windows clone with `core.autocrlf=true` checks text out with CRLF. `include_str!` then embeds
`lotml init`'s templates with CRLF, two CLI tests fail and the `pre-push` gate blocks the push; and
the corpus under `research/tokens/corpus/`, which is measured byte for byte, is measured with bytes
the repository does not hold. Done when a fresh Windows clone has LF in every text file and its CLI
tests pass.

## Paths

- `.gitattributes`

## Tasks

- [ ] 1.1 (Unit) Check out every text file with LF, binaries untouched

## Done when

- `git ls-files --eol` in a fresh clone with `core.autocrlf=true` lists no `w/crlf`, and the PNGs as `-text`.
- `cargo test --manifest-path compiler/Cargo.toml -p lotml --test cli` passes in that clone.
- `scc validate --checks --pr` exits 0.
