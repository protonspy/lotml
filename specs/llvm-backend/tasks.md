# LLVM backend — tasks

## 1 · The pipeline

- [x] 1.1 (Unit) Find `clang`, check its version, and compile a `.ll` file with the runtime into an executable, naming where it looked when none is found — R1.2, R1.3, R1.5
- [x] 1.2 (TDD) Emit functions, locals, numbers and `bool` with checked integer arithmetic, `/`, `//`, `%`, comparisons, and panics naming the `.lot` line — R2.1, R2.3, R2.4, R2.7
  _Depends 1.1, 1.6_
- [x] 1.3 (Unit) Emit `if`, `while`, `for` over `range`, `break`, `continue`, calls and recursion — R2.1
  _Depends 1.2_
- [x] 1.4 (Unit) Print numbers, `bool` and string literals through the runtime — R2.1, R2.5
  _Depends 1.2_
- [ ] 1.5 (Unit) Refuse every construct outside the first ones at the construct — R2.6
  _Depends 1.3_
- [x] 1.6 (Unit) Make every function of the runtime take the place it stops at as `const lt_at *`, the C target passing the address of its static sites — R2.7
  _Depends 1.1_

## 2 · On the command line

- [ ] 2.1 (Unit) Give `lotml run` and `build` the `--target llvm` option, `-O0` for `run` and `-O2` for `build` — R1.1, R1.4
  _Depends 1.4_
- [ ] 2.2 (Unit) Run `add`, `fib`, `collatz` and `mandelbrot` on `--target llvm` at both levels against the Python target's output, and check in `add`'s `.ll` — R2.2
  _Depends 2.1, 1.5_
