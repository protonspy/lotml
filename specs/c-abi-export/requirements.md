---
autonomy: auto
ci: wait
branch: feat/c-abi-export
delivery: in-review
pr: 32
---

# C ABI export — requirements

## Purpose

A LotML module as a library C can call: `lotml build --shared` writes a shared library and its C
header, exporting the functions whose signatures C and LotML can pass
(adr:0024-c-abi-exports-chosen-by-signature-without-new-syntax). It is for programs in C, C++,
Rust, Zig or any language with a C FFI that want a function written in LotML, compiled natively.

## R1 · The library

- **R1.1** Where `lotml build` is given `--shared`, the LLVM backend shall write a shared library and a C header for the module instead of an executable.
- **R1.2** The LLVM backend shall export from the library every top-level function other than `main` whose parameters and return type are integers, `f32`, `f64`, `bool` or `None`, or `str` among the parameters, under the symbol `<module>_<function>`.
- **R1.3** The LLVM backend shall declare each exported function in the header with the C types adr:0013-c-libraries-through-interfaces-named-c pairs with its LotML types, in a header that C11 and C++ compilers both accept.
- **R1.4** If a top-level function has a parameter or return type outside R1.2's, then the LLVM backend shall leave it out of the library and warn, naming the function and the type that excluded it.
- **R1.5** If no function of the module can be exported, then the LLVM backend shall stop with a diagnostic saying so.

## R2 · Calling it

- **R2.1** When an exported function is called from any thread, the library shall return what the same function returns when called from LotML, readying the runtime on the first call for calls from any thread, and leaving the host's standard streams as the host set them.
- **R2.2** If an exported function panics, then the library shall stop the process with status 101, naming the `.lot` file, line and function, as a LotML program does.
- **R2.3** If a `str` argument is not valid UTF-8, then the library shall stop the process with status 101, naming the function and the parameter.

## Out of scope

Returning `str`, records, collections, results or callbacks, which need an ownership rule
adr:0024 leaves for later. A library from a module that imports Python. A static library.
