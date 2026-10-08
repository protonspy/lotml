---
autonomy: auto
ci: wait
pr: per-group
merge: manual
status: approved
checksum: f1f569602dd63b12a3f58778d7e325151f37e20600d6e3fb3e25f11bb2cfd545
---

# C header import

Let `import c.<library>` work from the library's header alone: the compiler reads the header
through Clang and generates the interface, which today is written by hand. The types that cross
stay adr:0013's, and wider ones each come with an ownership rule of their own.

## Why

adr:0029-foreign-modules-are-imported-by-origin-and-their-interfaces-generated reverses
adr:0013-c-libraries-through-interfaces-named-c on one point: a C library's interface is
generated from its header, not written by hand. Zig's `@cImport` and Swift's Clang importer show
the way. Clang preprocesses the header for the target platform, so the macros and `#if`s that
adr:0013 gave as its reason become Clang's problem, and `lotml build` already requires `clang`.
Done when a program imports a C library with no hand-written `bindings/c.<library>.lotmli`, on
Windows and Linux, and the declarations the header holds outside the bound types are listed with
the reason.

## Paths

- `compiler/crates/lotml-check/` — the import of `c.<library>` and the interface a check reads
- `compiler/crates/lotml/src/exec.rs` — finding `clang` and the header for `check`
- `compiler/crates/lotml-py/tests/c_libraries.rs` — the hand-written interfaces in that test, which the header replaces
- `docs/adr/`

## References

- `specs/c-header-import/` — a header read through Clang into the interface of `c.<library>`
- adr:0029-foreign-modules-are-imported-by-origin-and-their-interfaces-generated
- adr:0013-c-libraries-through-interfaces-named-c
- adr:0027-lotml-provisions-a-pinned-llvm-toolchain-on-first-build
- `plans/python-compatibility.md` — the same import by origin, for Python

## Out of scope

- Pointers, structs, arrays, callbacks and varargs crossing: each needs an ownership rule first.
- C++ headers: templates, overloads and exceptions are a separate decision.
- Rust crates, reachable only through a C ABI they export themselves.

## Tasks

- [ ] 1.1 (Unit) Write and build the spec c-header-import: the header found for `c.<library>`,
  read through Clang's JSON AST under the check rules of adr:0029 (fixed arguments, closed
  search roots, timeout and size caps, Clang provisioned before a check as adr:0027 does for a
  build), the functions over adr:0013's types bound, and the rest listed as not bound with the
  reason
- [ ] 1.2 (Unit) Write a proposed ADR on a C struct passed by value, with the ownership rule it
  needs; the owner accepts it before 1.3
- [ ] 1.3 (Unit) Bind the structs passed by value that the ADR from 1.2 allows
  _Depends 1.1, 1.2_
- [ ] 1.4 (Unit) Bring the agent guide and `reference/lotml.md` to `import c.<library>` without a
  hand-written interface
  _Depends 1.1_

## Done when

- `lotml run` and `lotml build` run a program importing `c.m` with no `bindings/c.m.lotmli`, on
  Windows and Linux.
- The generated interface lists each declaration it does not bind, with the reason.
- `scc validate` exits 0.
