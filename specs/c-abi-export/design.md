# C ABI export — design

## What changes

Serves R1.1, R1.2, R1.4, R1.5.

`lotml build` gains `--shared`, valid only with the LLVM target. The LLVM backend compiles the
module as for an executable, without `@main`, and adds one wrapper per exported function; the
driver links with `clang -shared` (and `-fPIC` on Unix). On Windows that writes `<module>.dll`
and its import library `<module>.lib`; elsewhere `lib<module>.so` or `lib<module>.dylib`. The
header is `<module>.h`, beside it.

Which functions are exported is decided from the checker's signatures before lowering, so the
warning of R1.4 points at the declaration. `<module>` is the file's stem with every character
that cannot appear in a C identifier replaced by `_`.

## Boundaries and contracts

Serves R1.3, R2.1, R2.2, R2.3.

| LotML | in the header |
| --- | --- |
| `i8` … `i64`, `int`, `u8` … `u64` | `int8_t` … `int64_t`, `uint8_t` … `uint64_t` |
| `f32`, `f64` | `float`, `double` |
| `bool` | `bool` |
| `None` as the return type | `void` |
| `str` parameter | `const char *`, NUL-terminated UTF-8, read and copied, never kept |

The header includes `<stdint.h>` and `<stdbool.h>` and wraps its declarations in
`extern "C"` under `__cplusplus`; on Windows each declaration carries
`__declspec(dllimport)` unless the library itself is being built. Each wrapper is an `external`
LLVM function with `dllexport` on Windows; the functions it wraps stay `internal`, so the library
exports exactly R1.2's symbols.

A wrapper initialises the runtime once (an atomic flag, so a first call from two threads is
safe), converts each `str` argument into a runtime string — validating UTF-8 first — calls the
function, releases what it allocated, and returns. A panic inside names its function through
the constant place every site that can stop carries (`specs/llvm-backend/`); the runtime keeps
no stack of active functions, so the wrapper pushes nothing. Its narrow integer and `bool`
parameters and results carry the `signext` or `zeroext` `clang` gives the same C declaration,
so a caller built by gcc, clang or MSVC reads the same bits. A panic calls the same runtime
function a program's panic calls, which ends the process.

## Risks

- A C caller cannot catch a panic. A library whose functions can overflow or index out of range
  takes the process down on bad input; the header's comment for each function says that it may
  stop the process, and a `T ! E` return, the way to report failure instead, waits for the
  ownership rule (adr:0024).
- The runtime's state lives in the library. Two LotML libraries in one process each carry their
  own runtime; values never pass between them, so they do not interfere.
