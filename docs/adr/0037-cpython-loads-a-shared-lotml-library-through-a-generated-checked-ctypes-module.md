---
status: proposed
---

# 0037 · CPython loads a shared LotML library through a generated, checked ctypes module

## Context

Python can call LotML today only through the Python target. `lotml build --target python` writes
a module whose functions Python calls through wrappers that check and copy every value
(adr:0012-python-interop-through-checked-boundaries-and-interface-files). That code runs at
Python's speed. The fast path is the LLVM target, and `lotml build --shared` already writes a
shared library and a C header for it (specs/c-abi-export,
adr:0024-c-abi-exports-chosen-by-signature-without-new-syntax). The library exports every
function whose types C passes by value: integers, `f32`, `f64`, `bool`, `None` as `void`, and
`str` as a read-only, NUL-terminated parameter
(adr:0013-c-libraries-through-interfaces-named-c). Nothing loads that library from CPython.
adr:0012 covers only the Python target, and
adr:0023-native-programs-load-cpython-at-run-time-through-its-stable-abi is the other direction:
a native program loading CPython.

The prior art says the pieces are known and the pitfalls specific. LPython's `@lpython` compiles a
function and loads it into CPython, but it needs gcc, does not work on Windows, goes through
distutils and keeps a per-process cache (`research/prior-art/studies/lpython.md`, idea 6). The
plan that asked for this record (`plans/python-compatibility.md`, task 4.2) found the loader
planned nowhere. It also records two earlier proposals, neither in the repository:
- a native extension, `--pyext`, from an untracked review of the LLVM and IR work;
- that native code be loaded by an absolute path and confirmed by a handshake, from the security
  review of the interop.

Three constraints hold:
- The boundary must stay checked: adr:0012's promise is that no value of the wrong type reaches
  LotML code.
- Windows is a first-class host: CI builds and runs the LLVM target there (`llvm-windows`).
- A library's panic ends its process (specs/c-abi-export R2.2), because the runtime cannot unwind.

## Decision

Proposed, for the owner to rule on. `lotml build --shared` also writes `<module>_lotml.py` beside
the library, with a `.pyi` describing it, and a Python program imports that module. It requires
CPython 3.8 or later, so that Windows does not search the working directory or `PATH` for the
library's dependencies, and it checks `sys.version_info`.

**Loading.**
- The module opens the library once, with `ctypes.CDLL` and no `mode` or `winmode`, at
  `os.path.join(os.path.dirname(os.path.realpath(__file__)), name)`.
- It refuses to load unless that path is absolute. It never falls back to a bare name, which the
  system would search for.
- The library carries its runtime within it, so nothing else of LotML's is searched for.
- The handshake and every function use the one handle; the path is never opened twice.
- The loader trusts the permissions of that directory, and nothing more: whoever can write the
  library can write the module beside it.

**Handshake.**
- Before binding a function, the module calls `<module>_lotml_abi(buffer, capacity)`, which has
  fixed `argtypes` and `restype`. The function writes the runtime's ABI version and a hash of the
  exported signatures, and the module reads no more than the capacity it gave.
- The module refuses with an `ImportError` any answer that is not exactly the one it was generated
  with, and refuses a missing symbol as well. `lotml_abi` is a name a module's own function may
  not take.
- This detects a stale or mismatched build. It is not a check of integrity: a hostile library
  runs its initialisers when it is opened, before any question is asked, and can answer as
  expected.

**Calls.** Each exported function becomes a Python function that declares `argtypes` and
`restype`, converts each argument once, and checks it as adr:0012's wrappers do:
- **Integers.** An integer parameter takes an `int`, not a `bool`, converted once by
  `operator.index` and checked against its type's range, since ctypes wraps silently.
- **Floats.** A float parameter takes an `int` or a `float`, not a `bool`. An `f32` refuses a
  finite value that would round to infinity.
- **Booleans.** A `bool` parameter takes exactly `True` or `False`.
- **Text.** A `str` parameter takes exactly a `str`, encoded by `str.encode(s, "utf-8")`
  strictly. It refuses one holding `"\0"`, which the C ABI's NUL-terminated string cannot carry.
- **Raw access.** The raw `CDLL` handle and unwrapped functions are not exported.
- **Threads.** ctypes releases the GIL during a call. A `str` argument is safe to free after the
  call returns, because the library copies it and keeps none (specs/c-abi-export).
- **Generated source.** The generator writes LotML identifiers, checked against Python's
  keywords, and any text as a `repr`.

**Ruled out:**
- **A native CPython extension (`--pyext`).** LLVM would emit `PyInit_` and argument parsing
  against CPython's stable ABI. That is faster per call, but it ties the backend to CPython's C
  API and its versions. It is a larger change than the gap it fills.
- **`cffi`.** A dependency the project does not carry (`docs/stack.md`), for what the standard
  library's ctypes already does.
- **No loader, users writing ctypes by hand.** This drops adr:0012's checks exactly where a wrong
  type corrupts native memory.

## Consequences

- Python gets LotML's native speed for the types adr:0013 passes by value, on every host with no
  compiler at load time.
- A call costs ctypes' overhead, which is of the order of a microsecond but not measured here. So
  a function worth loading is one that does real work per call, and a native extension stays the
  answer if the overhead is measured to matter.
- A stale or mismatched library is refused at import. A hostile one is not: the directory's
  permissions are the boundary. A file's SHA-256 written into the module would add an integrity
  check only where the module is better protected than the library.
- A panic in the library ends the Python process with status 101 and prints where it happened,
  including the build's paths. Python's `finally` blocks, `atexit` handlers and buffered files do
  not run.
  - On the Python target the same bug is an `Err(PyError)`. So input a service does not trust
    should be checked before the call, or the call made in a subprocess.
  - Raising the panic as a Python exception needs an unwinding boundary the runtime does not
    have. A call runs to the end, and cannot be interrupted while it runs.
- Whether two Python threads may run one library's functions at once is shown for the runtime's
  first call and its output buffer (specs/c-abi-export R2.1). Calls that share any other state of
  the runtime must be tested before this is accepted.
- Returning `str`, passing records, collections or callbacks waits for the ownership rule that
  adr:0013 and adr:0024 already record as their ceiling.
- A library from a module that imports Python stays out. specs/c-abi-export leaves it out of
  scope, and the LLVM build refuses such a module (E0401).
