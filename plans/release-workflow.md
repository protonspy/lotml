---
autonomy: auto
ci: wait
pr: per-plan
merge: manual
status: approved
checksum: bff846e445912dc95e53b81a16788b90f499199b1c7a3d15e782c68d8bca9bc9
---

# Release workflow

A GitHub Actions workflow, started by hand, that builds the `lotml` compiler for Linux, Windows
and macOS, packages the VS Code extension, and publishes them as a GitHub release tagged with the
workspace's version.

## Why

`lotml` is installed today by building it from a checkout, with Rust 1.97, which is the step
between a reader of the README and a first `lotml check`. A release built by CI gives a
download that needs nothing installed: the compiler embeds its Python and C runtimes and the
guide `init` writes. Done when the workflow is on `main`, lints clean, refuses what it should,
and the README says where releases are.

## Paths

- `.github/workflows/release.yml`
- `README.md`, `docs/stack.md`

## Out of scope

- Running the workflow: a run publishes a release, and that is the user's call.
- Publishing the extension to the VS Code Marketplace, or the compiler to crates.io.
- Signing or notarising the binaries, a statically linked Linux build, and macOS on Intel.
- Choosing the version: it is `[workspace.package] version` in `compiler/Cargo.toml`, bumped by a
  commit before a release.

## Tasks

- [x] 1.1 (Unit) Write `.github/workflows/release.yml`: `workflow_dispatch` from `main` only, with a `prerelease` input; the version read from the workspace, refused when its `v<version>` tag exists; `lotml` built `--locked --release` for `x86_64-unknown-linux-gnu`, `x86_64-pc-windows-msvc` and `aarch64-apple-darwin`, each archived with the README and the licence; the `.vsix` built from `editors/vscode`; a release created on the run's commit with every archive, the `.vsix` and a `SHA256SUMS`
- [x] 1.2 (Unit) Say in the README where releases are and what an archive holds, and list GitHub Actions in `docs/stack.md`
  _Depends 1.1_

## Done when

- `actionlint` reports nothing on `.github/workflows/`.
- `scc validate` reports no findings, and the PR's CI is green.
