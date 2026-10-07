---
autonomy: auto
ci: wait
---

# Python bridge — requirements

## Purpose

Native programs importing Python modules: "native when possible, Python when necessary". A
program built on the LLVM target calls the functions of a Python module through its interface
(adr:0012-python-interop-through-checked-boundaries-and-interface-files) by loading CPython when
it starts (adr:0023-native-programs-load-cpython-at-run-time-through-its-stable-abi), with the
results the Python target gives. It lifts the native targets' refusal of Python imports.

## R1 · Calling Python

- **R1.1** Where a program imports a Python module through its interface, the LLVM backend shall compile it, calling the module's functions through CPython.
- **R1.2** When a bound function is called, the LLVM backend's program shall convert each argument to a Python value, call the function, and return `Ok` of the result converted to the declared type.
- **R1.3** If the call raises, or returns a value that does not fit the declared type, then the LLVM backend's program shall return `Err(PyError(kind, message))` with the kind and message the Python target returns for the same call.
- **R1.4** The LLVM backend's program shall convert, in both directions and by copy, the integers, `f32`, `f64`, `bool`, `str`, `None`, and optionals, lists, tuples, dicts and sets of these.
- **R1.5** When tasks of `parallel` call Python, the LLVM backend's program shall hold CPython's global interpreter lock only while each call runs.
- **R1.6** When a program finishes in a test build, the LLVM backend's program shall have released every Python object it created, which the test build reports.

## R2 · Finding CPython

- **R2.1** When `lotml build` compiles a program that imports a Python module, the LLVM backend shall write into the executable the CPython shared library and module search path of the Python it finds.
- **R2.2** When such a program starts, the LLVM backend's program shall load the CPython library `LOTML_PYTHON_LIB` names, or else the recorded one, and import each module the program imports, before `main` runs.
- **R2.3** If the CPython library cannot be loaded, or a module does not import, then the LLVM backend's program shall stop with status 101, naming the library or module and where it looked.
- **R2.4** If no Python is found at build time, or the one found is older than 3.10 or free-threaded, then the LLVM backend shall stop with a diagnostic naming the reason and `LOTML_PYTHON`.
- **R2.5** While a program imports no Python module, the LLVM backend's program shall neither load nor need CPython.

## Out of scope

Python classes, callbacks from Python into LotML, and types beyond R1.4, which adr:0012 records as
ceilings. Free-threaded CPython (adr:0023). The C target, retired before this spec is built (plan
task 2.5).
