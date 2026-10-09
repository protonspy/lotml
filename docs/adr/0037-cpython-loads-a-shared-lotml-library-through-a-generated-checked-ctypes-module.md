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
`str` as a read-only parameter (adr:0013-c-libraries-through-interfaces-named-c). Nothing loads
that library from CPython. adr:0012 covers only the Python target, and adr:0023-native-programs-load-cpython-at-run-time-through-its-stable-abi is the other
direction: a native program loading CPython.

The prior art says the pieces are known and the pitfalls specific. LPython's `@lpython` compiles
a function and loads it into CPython, but it needs gcc, does not work on Windows, goes through
distutils and keeps a per-process cache (`research/prior-art/studies/lpython.md`, idea 6). The
review of the LLVM and IR work proposed a native extension (`--pyext`) and deferred it to a plan
of its own. A security review of the interop asked that native code be loaded by an absolute
path, never by a search, and only after a handshake confirms the library is the one the caller
was generated for.

Three constraints hold:
- The boundary must stay checked: adr:0012's promise is that no value of the wrong type reaches
  LotML code.
- Windows is a first-class host: CI builds and runs the LLVM target there (`llvm-windows`).
- A library's panic ends its process (specs/c-abi-export R2.2), because the runtime cannot unwind.

## Decision

Proposed, for the owner to rule on. `lotml build --shared` also writes `<module>_lotml.py` beside
the library, with a `.pyi` describing it, and a Python program imports that module.

**Loading.** The module loads the library with `ctypes.CDLL` by the absolute path of the file
next to it (`os.path.dirname(__file__)`). It never searches `PATH`, `LD_LIBRARY_PATH` or the
working directory.

**Handshake.** Before it binds a function, the module calls a function the library exports,
`<module>_lotml_abi`. That function returns the runtime's ABI version and a hash of the exported
signatures. The module refuses to load, with an `ImportError` naming both sides, a library whose
answer differs from the one it was generated with.

**Calls.** Each exported function becomes a Python function that declares `argtypes` and
`restype` and checks its arguments as adr:0012's wrappers do. An integer must be in its type's
range, a `float` must be a number, a `bool` must be a `bool`, and a `str` must be text, encoded to
UTF-8 by the wrapper. ctypes releases the GIL during the call. The library is safe to call from
any thread (specs/c-abi-export R2.1).

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
  compiler at load time. A call costs ctypes' overhead, about a microsecond, so a function worth
  loading is one that does real work per call. A native extension stays the answer if that
  overhead is measured to matter.
- A stale or swapped library is refused at import, never called with the wrong signatures.
- A panic in the library ends the Python process with status 101, as it ends a C host. Raising it
  as a Python exception needs an unwinding boundary the runtime does not have.
- Returning `str`, passing records, collections or callbacks waits for the ownership rule that
  adr:0013 and adr:0024 already record as their ceiling.
- A module whose library imports Python stays out, as specs/c-abi-export keeps it out (E0401).
