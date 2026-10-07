# LLVM backend — design

## What changes

Serves R1.1, R2.1, R2.2.

A new crate, `compiler/crates/lotml-llvm`, and `llvm` as a value of the CLI's `Target`. The crate
reads `lotml_ir::native`'s output and writes one `.ll` file; it links nothing from LLVM.

```
counted IR (lotml-ir)
  └─ emit.rs    one walk per function: structured statements to basic blocks, textual LLVM IR
runtime crate (plan task 1.1)   the C runtime, compiled by the same clang
driver.rs   finds clang, checks its version, compiles .ll + runtime, links, runs
```

**Structured to blocks.** Each IR function becomes one `define`. A local becomes an `alloca` in
the entry block, read and written with `load`/`store`; `-O2`'s `mem2reg` turns them into SSA
values, so the emitter builds no phi nodes. `If` writes a then-block, an else-block and a join;
`Loop` writes a header and an exit, with a stack of (continue, break) targets for `Continue` and
`Break`; `ForRange` is a loop over a counter with its `exit` block after the test fails. A block
that ends in `ret`, `br` or `unreachable` takes no fall-through branch.

**Types.** Integers are `iN`, the signedness decided per operation (`sdiv`/`udiv`, `icmp
slt`/`ult`, `sext`/`zext`); `f64` is `double`, `f32` `float`, `bool` `i1`. Pointers are written in
the opaque `ptr` form, which sets the oldest supported `clang` at 17; the driver reads
`clang --version` and refuses older ones (R1.3).

**Checks and panics.** `+`, `-` and `*` on integers call `llvm.{s,u}{add,sub,mul}.with.overflow`
and branch on the flag to a cold block that calls the runtime's panic function, then
`unreachable` (R2.3, R2.4). Division tests the divisor, and the `INT_MIN / -1` case, before
dividing. `//` and `%` round toward negative infinity, as the C target writes them. All of this
is written in IR, not called: the C target reaches it through `static inline` functions of
`lotml.h` (`lt_add_i64`, `lt_floordiv_i64`, …), which no object file exports (n-0079).

**Where a panic happened.** The C target names the place with `LT_HERE`, an `lt_at {file,
line, function}` built from `#line`, `__LINE__` and the `lt_fn` every generated C function
declares; the runtime keeps no stack of active functions. The emitter writes one constant
`lt_at` global per site that can stop, from the statement's span and the function's LotML name,
and passes its address (n-0080).

**The boundary with the runtime.** The runtime's functions take `lt_at` by value — 112
declarations in `lotml.h` — and LLVM does not lower an aggregate argument to the platform's C
ABI: `clang` passes that 24-byte struct through a hidden pointer on Win64 and `byval` on SysV, so
a call written with the struct as a value reads the wrong bytes (n-0078). So every function of
the runtime takes the place by pointer, `const lt_at *` — one API for both native backends, the
C target passing the address of the static site it already declares (`&lt_site_<line>`) —
and the rule is general (R2.7): what crosses into the runtime is a scalar or a pointer. Integers narrower than 32 bits and `bool` cross with the `signext` or `zeroext`
`clang` gives them.

**Printing.** `print` calls the runtime functions the built-in table names for each type, so a
float is written by the runtime's CPython `repr`, never by `printf` (R2.5). A string literal goes
to the runtime as a pointer to its bytes and a length, so this increment depends on no cell
layout; static string cells arrive with `specs/llvm-parity/`'s layout test.

**Entry.** The emitter writes `define i32 @main()`, which initialises the runtime and calls the
program's `main`, returning the exit status `lotml run` expects.

## Driving `clang`

Serves R1.2, R1.3, R1.4, R1.5.

`clang` is the first of `LOTML_CLANG`, `clang` on `PATH`, and on Windows
`%ProgramFiles%\LLVM\bin\clang.exe`, which the LLVM installer writes without adding to `PATH`.
adr:0021 names the variable and `PATH`; the fixed install directory is added here for the same
reason `find-msvc-tools` locates `cl`, and it is never the working directory. The command is
`clang <prog>.ll <runtime>.c… -o <exe> -O0|-O2`, plus `-lm` and threads on Unix; on Windows
`clang` drives MSVC's linker and the Windows SDK, and a link that fails for want of them is
reported with that cause. A `clang` error on the `.ll` keeps the file and reports R1.5's message.

## Risks

- Textual IR is not checked before `clang` reads it (adr:0021), so the tests compile and run
  everything the emitter writes; `add` keeps a checked-in `.ll` to compare against, so a change
  to the emitter shows in review.
- `-O0` and `-O2` must agree. The end-to-end tests run each program at both levels.
- Tests skip, saying so, on a machine without `clang`; CI installs it on Ubuntu and Windows so
  they never skip there.
