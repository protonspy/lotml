# The frontend's fuzzer

cargo-fuzz over libFuzzer, on nightly, outside the workspace
(adr:0027-fuzz-the-frontend-with-cargo-fuzz-on-nightly). The `frontend` target takes any UTF-8 text
through the checker, the prefix checker and the compile of both targets; an input that panics is a
bug.

On Linux or macOS, or under WSL on Windows (libFuzzer does not link with MSVC), from `compiler/`:

```sh
rustup toolchain install nightly --profile minimal
cargo install cargo-fuzz --locked
cargo +nightly run --manifest-path fuzz/Cargo.toml --bin seed   # the corpus's programs, as first inputs
cargo +nightly fuzz run frontend -- -max_total_time=600         # ten minutes
```

From WSL on a Windows checkout, keep the build on the Linux filesystem, which is much faster:
`export CARGO_TARGET_DIR=$HOME/lotml-fuzz-target` first.

A crash is written to `fuzz/artifacts/frontend/`; `cargo +nightly fuzz run frontend <file>` replays
it. Fix it where it panics, and add the input as a test of that crate.
