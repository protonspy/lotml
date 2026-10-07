---
autonomy: auto
ci: wait
pr: per-group
merge: manual
status: approved
checksum: bb0e82b47063948fb9b9816dfdb7053801cc351a979d8a09d4d62a9d0ee5443f
---

# Native toolchain

Provision a pinned LLVM toolchain on the first `lotml build`, so a native build needs no LLVM,
and on Windows no Visual Studio, installed by the user:
- x86_64 Windows: llvm-mingw;
- x86_64 Linux: clang and lld, with the system's libc;
- arm64 macOS: clang, with Apple's `ld` and the Command Line Tools' SDK.

## Why

`lotml build` needs a clang the user installed, and on Windows the MSVC toolchain. Those are
gigabytes, and the clang version varies by machine.

adr:0027-lotml-provisions-a-pinned-llvm-toolchain-on-first-build decides the fix:
- archives from a dedicated toolchain release of this repository, pinned by hashes committed
  here;
- downloaded and unpacked by absolute-path system tools into a checked per-user directory;
- clang run with no default configuration, from a private directory, with a scrubbed
  environment;
- opt-in until the parity suite passes on it;
- never downloaded by the MCP server, the grader or the harness.

Done when a fresh runner of each target, with only the release archive and the stated system
requirements, runs `lotml build`, and the parity suite and the C ABI tests pass on the
provisioned toolchain, which is then the default.

## Paths

- `.github/workflows/`
- `.github/CODEOWNERS`
- `compiler/crates/lotml-llvm/src/`
- `compiler/crates/lotml-llvm/tests/`
- `compiler/crates/lotml-runtime/`
- `compiler/crates/lotml/src/exec.rs`
- `compiler/crates/lotml/src/main.rs`
- `compiler/crates/lotml/src/mcp.rs`
- `harness/lotml_harness/`
- `specs/`
- `README.md`
- `docs/stack.md`
- `docs/wiki/pages/transpilation-strategy.md`

## References

- adr:0027-lotml-provisions-a-pinned-llvm-toolchain-on-first-build — the decision this builds
- adr:0021-compiler-in-rust-with-llvm-as-its-native-code-generator, adr:0022-lotml-build-makes-a-native-executable-by-default — refined by adr:0027
- adr:0024-c-abi-exports-chosen-by-signature-without-new-syntax — `--shared` keeps its C ABI on mingw
- adr:0026-lotml-ships-uv-and-runs-python-3-14-by-default — the same offline and lookup rules, for Python
- `plans/python-via-uv.md` — the offline switch this reuses
- `plans/build-and-check-speed.md` — its runtime object cache and SHA-256 code, which this reuses

## Out of scope

- Shipping the toolchain inside lotml's release archive.
- Targets other than x86_64 Windows, x86_64 Linux and arm64 macOS, and cross-compiling.
- Redistributing Apple's SDK and linker or Microsoft's linker and libraries.

## Tasks

- [ ] 1.1 (Unit) Add a workflow that builds the toolchain archives under a `toolchain-llvm<version>-<n>` tag:
  - it checks LLVM's archives against its signatures, and llvm-mingw's against a hash committed after checking by hand;
  - it trims by selecting files (clang, lld, resource headers, compiler-rt, `llvm-dwarfdump`, `llvm-objdump`, `llvm-readobj`, `llvm-nm`, link libraries), turning links into copies;
  - it re-signs macOS binaries ad hoc;
  - it writes each archive deterministically, with a per-component licence inventory;
  - its build jobs hold no secrets, and the publishing job runs no downloaded binary.

  A listing script fails CI on a non-regular entry, an absolute or `..` path, a Windows reserved name, a missing licence text, or any GPL or LGPL file.
- [ ] 1.2 (Unit) Commit each target's archive version, SHA-256, size and URL in one file compiled into lotml, the URL built from a fixed host and path, with the file under CODEOWNERS review
  _Depends 1.1_
- [ ] 1.3 (TDD) Download and unpack. Reuse the SHA-256 that `plans/build-and-check-speed.md` adds, tested against NIST vectors and a streamed file over 200 MB:
  - run System32's `curl.exe` and bsdtar `tar.exe`, or `/usr/bin` and `/bin`, by absolute path;
  - give curl no configuration file, HTTPS only, a size limit and timeouts;
  - refuse while TLS verification is disabled by a variable;
  - verify, then unpack the verified bytes in a fresh unpredictable directory inside the owner-only toolchain directory, without owners, permissions or attributes;
  - rename into a directory named by version and hash, and write the marker last.

  Tests: a wrong hash, a truncated download, link, `..` and absolute entries, a planted or group-writable directory, a missing marker, and two builds racing.
  _Depends 1.2_
- [ ] 1.4 (Unit) Behind an opt-in switch, find clang in the order adr:0027 sets, by absolute path only:
  - `LOTML_CLANG`, now absolute;
  - the provisioned toolchain with its marker;
  - a download unless offline, falling through on failure;
  - an explicit path search;
  - the LLVM installer's location.

  Record in the JSON of `lotml build` the clang's path, version and origin, and update `driver::missing` to name the toolchain, the download and the offline switch.
  _Depends 1.3_
- [ ] 1.5 (Unit) Run the provisioned clang with no default configuration, the archive's linker by absolute path, from lotml's private build directory, and an environment cleared of the variables that rewrite a compiler's command line or search paths. Confirm that a `c.<library>` name can only be an identifier. A CI test with a poisoned path and a project directory holding a fake library checks with `-###` that every subprocess resolves inside the toolchain
  _Depends 1.4_
- [ ] 1.6 (Unit) Make the provisioning function unreachable from the MCP server, the grader and the harness, with a test through an injected downloader that they make no network request and write nothing to the toolchain directory
  _Depends 1.4_
- [ ] 2.1 (Unit) Build and link with llvm-mingw on Windows: the runtime, executables, and `--shared` DLLs with lld's import library. Run the LLVM tests, with their line tables read as DWARF, and the C ABI export tests through `LOTML_CLANG`, and write the deltas of the c-abi-export and llvm-backend specs
  _Depends 1.1_
- [ ] 2.2 (Unit) Run the LLVM tests on the provisioned clang and lld on Linux, naming the missing C library and GCC runtime packages, and on macOS with `/usr/bin/xcrun`'s SDK and `ld`, naming the missing Command Line Tools
  _Depends 1.1_
- [ ] 2.3 (Unit) Confirm which mingw-w64 runtime pieces end up inside programs lotml builds, and what notices that requires of them
  _Depends 2.1_
- [ ] 2.4 (Unit) Key the runtime object cache on the provisioned toolchain's pinned hash, and on a hash of the binary for any other clang, once `plans/build-and-check-speed.md` has landed its cache
  _Depends 1.4_
- [ ] 2.5 (Unit) Run the parity suite on each target's provisioned toolchain, then make steps 2 and 3 of the lookup the default
  _Depends 1.5, 1.6, 2.1, 2.2, 2.3_
- [ ] 3.1 (Unit) Add CI jobs on Windows, Linux and macOS that start from the release archive on a runner with no LLVM or Visual Studio on the path. Each runs `lotml build` once with the download and once offline, and checks that the offline run fails naming what is missing
  _Depends 2.5_
- [ ] 3.2 (Unit) Describe the provisioned toolchain, the system requirements per target, the lookup order, the offline switch, the Windows move to mingw-w64 and the absolute `LOTML_CLANG` in `README.md` and `docs/wiki/pages/transpilation-strategy.md`, and replace the clang entry of `docs/stack.md` with what the code now does
  _Depends 3.1_

## Done when

- `lotml build` on a fresh runner of each target downloads the toolchain once, saying from where, and builds.
- With the offline switch and no toolchain, `lotml build` fails naming what is missing and downloads nothing.
- The parity suite and the C ABI export tests pass on each provisioned toolchain, which is then the default.
- `cargo test`, clippy, `uv --directory harness run pytest` and ruff pass, and `scc validate` exits 0.
