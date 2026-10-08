---
autonomy: auto
ci: wait
branch: feat/llvm-backend
delivery: merged
pr: 31
---

# LLVM backend — requirements

## Purpose

The first increment of `--target llvm` (adr:0021-compiler-in-rust-with-llvm-as-its-native-code-generator):
textual LLVM IR written from the counted IR (`specs/shared-ir/`) and compiled by `clang` with the
runtime, for numbers, `bool`, control flow, calls and printing. It is done when `add`, `fib`,
`collatz` and `mandelbrot` print on `--target llvm` what they print on the Python target, which
proves the pipeline end to end before `specs/llvm-parity/` takes it to the whole language.

## R1 · Finding and driving `clang`

- **R1.1** When `lotml run` or `lotml build` is given `--target llvm`, the LLVM backend shall write LLVM IR from the counted IR, compile it with the runtime by `clang`, and run the program or write the executable.
- **R1.2** The LLVM backend shall look for `clang` in `LOTML_CLANG`, then on `PATH`, then, on Windows, in the LLVM installer's directory, never in the working directory, and shall pass it its arguments as a vector, never through a shell.
- **R1.3** If no `clang` is found, or the one found is older than the oldest version the backend supports, then the LLVM backend shall stop with a diagnostic naming where it looked, the version it needs, and `--target python`.
- **R1.4** When `lotml build` compiles, the LLVM backend shall optimise with `-O2`; when `lotml run` compiles, with `-O0`.
- **R1.5** If `clang` rejects the IR the backend wrote, then the LLVM backend shall stop with a message that calls it a compiler bug, quotes `clang`'s first error and names the `.ll` file it kept.

## R2 · The first constructs

- **R2.1** The LLVM backend shall compile functions, locals, `var`, the integers `i8` to `u64`, `f32`, `f64`, `bool`, arithmetic, comparisons, `if`, `while`, `for` over `range`, `break`, `continue`, calls, recursion, and `print` of these values and of string literals.
- **R2.2** When a compiled program runs, the LLVM backend's program shall write to standard output exactly what the Python target's program writes, and exit with the same status.
- **R2.3** The LLVM backend shall check every integer operation and conversion against the range of its type with LLVM's checked-arithmetic intrinsics, and give `/`, `//` and `%` the meaning adr:0007-integer-division-returns-f64 gives them.
- **R2.4** If an integer operation overflows, a divisor is zero, an `assert` fails or `todo()` runs, then the LLVM backend's program shall stop with status 101 and name the `.lot` file, line and function where it stopped.
- **R2.5** When a float is printed, the LLVM backend's program shall print it as CPython's `repr` does.
- **R2.6** If a program uses a construct R2.1 does not list, then the LLVM backend shall refuse it with a diagnostic at the construct that names `--target llvm` and the targets that compile it.
- **R2.7** The LLVM backend shall pass to the runtime, and take from it, only scalars and pointers, every aggregate through a pointer, with integers narrower than 32 bits and `bool` extended as `clang` declares them.

## Out of scope

Strings beyond literals, collections, records, sum types, closures, generics, counting, `test`
blocks and debug information: `specs/llvm-parity/`. Linking LLVM's libraries, rejected by
adr:0021. Making `--target llvm` the default of `build`: plan task 2.3.
