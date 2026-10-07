---
status: accepted
---

# 0027 · lotml provisions a pinned LLVM toolchain on the first build

## Context

`lotml build` compiles textual LLVM IR and the C runtime with `clang` 17 or later
(adr:0021-compiler-in-rust-with-llvm-as-its-native-code-generator,
adr:0025-two-targets-python-for-run-llvm-for-build). `lotml-llvm/src/driver.rs` finds clang
through `LOTML_CLANG`, then the path, then where the LLVM installer for Windows puts it. On
Windows the result links through the MSVC toolchain, whose linker, CRT and Windows SDK neither
ship with lotml nor can be redistributed.

So a user who wants a native build installs LLVM and, on Windows, Visual Studio's build tools,
which are gigabytes. The clang version is whatever the machine has, so native output and the
parity suite vary between machines.

adr:0026-lotml-ships-uv-and-runs-python-3-14-by-default removed the same problem for Python. The
owner chose to do the same for the native toolchain: LLVM per platform, downloaded on the first
build rather than carried in the archive. This refines adr:0021 and
adr:0022-lotml-build-makes-a-native-executable-by-default. clang is still found and never linked
into lotml, but a machine without it now gets one, unless lotml is offline.

## Decision

**What is provisioned, for the three targets lotml releases:**
- **x86_64 Windows:** llvm-mingw, which is clang, lld and mingw-w64. It needs no Visual Studio
  and no Windows SDK.
- **x86_64 Linux:** clang and lld from an official LLVM release, linking against the system's
  libc. The C library's development files and the GCC runtime objects (`libc6-dev` and `libgcc`
  or their equivalents) are a stated requirement, and lotml names them when they are missing.
- **arm64 macOS:** clang from an official LLVM release, linking with Apple's `ld` and the SDK of
  the Command Line Tools, both found through `/usr/bin/xcrun`. They cannot be redistributed, so
  they are a stated requirement.

Every archive carries what the build and its tests use:
- clang and lld, and the compiler's resource headers;
- compiler-rt, for `LOTML_SANITIZE`;
- `llvm-dwarfdump`, `llvm-objdump`, `llvm-readobj` and `llvm-nm`.

Other architectures are out of scope until a decision adds them.

**The archives come from a dedicated release of lotml's repository.**
- They are published under a toolchain tag (`toolchain-llvm<version>-<n>`), independent of
  lotml's own version, and that release is never deleted or edited.
- A workflow builds them from the pinned upstream archives:
  - LLVM's are checked against its signatures;
  - llvm-mingw's against a hash committed in the repository after being checked by hand.
- The trim selects files and recompiles nothing. It turns symbolic and hard links into copies,
  so an archive holds only regular files and directories.
- The tar step is deterministic (sorted entries, fixed times and owners, fixed compression), so
  anyone can rebuild an archive and compare it.
- The build jobs hold no secrets and only read the repository. The one job that publishes never
  runs a downloaded binary.

lotml's source then commits each archive's SHA-256, size and URL, built from a fixed host and
path. A pin moves only by a pull request that changes them together, reviewed by the repository's
owners.

**Downloading and unpacking.**
- lotml runs curl and tar by absolute path: System32's `curl.exe` and its bsdtar `tar.exe` on
  Windows 10 1803 or later, and `/usr/bin` or `/bin` on Linux and macOS.
- curl ignores its configuration files and gets HTTPS only, redirects included, with a size limit
  and timeouts. lotml refuses to download while a variable that disables TLS verification is set.
- The archive goes into a fresh, unpredictable directory inside lotml's per-user toolchain
  directory. That directory is owner-only, and lotml refuses it if it is a link or writable by
  others.
- lotml checks the SHA-256 and rejects any entry that is not a regular file or directory or
  that would land outside. It unpacks the verified bytes without restoring owners, permissions
  or extended attributes, then renames the result into a directory named by version and hash.
- A marker holding the hash is written last; a directory without it is incomplete and ignored.
  A concurrent build that loses the rename uses the winner's directory after checking its
  marker. On macOS, the binaries are re-signed ad hoc after trimming.
- Before downloading, lotml prints the version, the size and the URL. The toolchain directory
  has no override by environment.

**Lookup, by absolute path only.**
1. `LOTML_CLANG`, an existing file named by an absolute path, which keeps linking through MSVC
   available on Windows;
2. the provisioned toolchain, if its marker checks;
3. a download, unless offline. A failed download falls through to the next step, saying so;
4. clang on the path, searched explicitly, skipping relative and empty entries and the current
   directory, then the LLVM installer's location on Windows.

Steps 2 and 3 are opt-in until the parity suite and the C ABI tests pass on each target's
provisioned toolchain; then they become the default. lotml records which clang built a program,
its version and where it came from.

**Running the provisioned clang.**
- It ignores default configuration files.
- It uses the archive's own linker by absolute path.
- It runs from lotml's private build directory, not the project.
- Its environment is cleared of the variables that rewrite a compiler's command line or search
  paths.

**The MCP server, the grader and the harness never download.** They are offline in code, as
adr:0026 decides for Python: the provisioning function is unreachable from them. They use a
toolchain already provisioned or clang from steps 1 and 4, or they fail naming what is missing.

Rejected:
- **Carrying the toolchain in the release archive:** over 200 MB per platform for every user,
  including those who never build natively.
- **Zig as the toolchain:** one package for every target, but its acceptance of textual LLVM IR
  is unproven, and its move away from LLVM makes it a moving base.
- **Only checking for an installed clang:** it leaves the setup and the variation that motivated
  this decision.

## Consequences

- **The first native build downloads.** It fetches the toolchain, whose size is measured once it
  is trimmed and recorded in the pin, and says so. Later builds and the offline switch use the
  copy already there.
- **On Windows the provisioned default is mingw-w64.**
  - Native programs link the mingw-w64 C runtime, and `-g` writes DWARF rather than CodeView.
  - A `--shared` library is a mingw-built DLL. Its C ABI exports
    (adr:0024-c-abi-exports-chosen-by-signature-without-new-syntax) are called from C as before,
    and its import library is lld's.
  - `specs/c-abi-export` and `specs/llvm-backend` gain a delta for it.
  - A user who needs MSVC linking keeps it with `LOTML_CLANG`.
- **`LOTML_CLANG` must now be an absolute path.** A bare name no longer resolves through the
  path.
- **Licences.** lotml redistributes the toolchain, so each archive carries a per-component
  licence inventory: LLVM's Apache-2.0 with the LLVM exception and its notices, and mingw-w64's
  ZPL, BSD and MIT-style notices. The release notes link the exact upstream sources. No GPL or
  LGPL file is kept. Which mingw-w64 runtime pieces end up inside programs lotml builds, and the
  notices that then reach them, is checked before the Windows default flips.
- **The runtime object cache keys on the toolchain's pinned hash**
  (`plans/build-and-check-speed.md`).
- **Native builds become reproducible**, apart from the system pieces Linux and macOS still
  supply: the same lotml version builds with the same clang on every machine of a target.
