---
status: accepted
---

# 0024 · C ABI exports chosen by signature, without new syntax

## Context

C can call nothing written in LotML: `lotml build` writes an executable, or a Python module whose
functions Python calls through checked wrappers
(adr:0012-python-interop-through-checked-boundaries-and-interface-files). Exporting functions
through the C ABI makes a LotML module a library for C, C++, Rust, Zig and any language with a C
FFI. Something has to say which functions are exported. The syntax is frozen
(adr:0010-variant-b-and-indented-blocks-settled-by-the-phase-0-gate), LotML has no visibility
marker, and adr:0013-c-libraries-through-interfaces-named-c already settled which types cross
between C and LotML without an ownership rule: integers, `f32`, `f64`, `bool`, `None` as `void`,
and `str` as a read-only parameter. Anything else — a list, a record, a result — needs a rule for
who frees it, which the value semantics do not have yet.

## Decision

`lotml build --shared` writes a shared library and a C header. It exports every top-level
function of the module, other than `main`, whose parameters and return type are all types
adr:0013 lets cross, under the symbol `<module>_<function>`. Every other top-level function is
left out with a warning naming it and the type that excluded it, and a module with nothing to
export is an error. A panic inside an exported function stops the process with status 101, as it
stops a LotML program. Rejected: an `export` keyword or decorator, which changes the frozen
syntax; a separate file listing the exports, a second record of each signature to keep in step;
and exporting every function with opaque handles for the types that cannot cross, which is the
ownership rule adr:0013 records as a ceiling.

## Consequences

- Which functions a library exports follows from their signatures, so changing a parameter to a
  list silently drops a function from the library; the warning is what shows it.
- The symbol prefix keeps two LotML libraries linked into one program from colliding.
- Returning strings, passing records and calling back into C wait for an ownership rule, the
  same ceiling adr:0013 recorded for the other direction.
