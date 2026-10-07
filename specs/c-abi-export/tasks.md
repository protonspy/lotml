# C ABI export — tasks

## 1 · Choosing and declaring

- [x] 1.1 (Unit) Choose the exported functions from the checker's signatures, warning for each one left out and stopping when none is left — R1.2, R1.4, R1.5
- [x] 1.2 (Unit) Write the C header for the chosen functions, compiled as C11 and as C++ in a test — R1.3
  _Depends 1.1_

## 2 · The library

- [x] 2.1 (Unit) Emit a wrapper per exported function that readies the runtime for calls from any thread on the first call, converts `str` arguments with UTF-8 validation and names the function in panics — R2.1, R2.2, R2.3
  _Depends 1.1_
- [x] 2.2 (Unit) Give `lotml build` the `--shared` option, linking a shared library on Windows and Linux — R1.1
  _Depends 2.1, 1.2_
- [x] 2.3 (Unit) Build a C program against the library and header, call the exported functions from two threads, and check their results and a panic's status — R2.1, R2.2
  _Depends 2.2_
