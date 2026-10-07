# Python bridge — design

## What changes

Serves R1.1, R1.2, R2.2, R2.5.

Three places, the decision itself recorded in
adr:0023-native-programs-load-cpython-at-run-time-through-its-stable-abi:

- **The runtime crate** gains `lotml_py.c`, compiled into a program only when it imports a Python
  module (R2.5). It loads the library (`LoadLibraryExW` with `LOAD_LIBRARY_SEARCH_DEFAULT_DIRS`
  plus the library's own directory on Windows, `dlopen` with `RTLD_NOW | RTLD_GLOBAL` elsewhere),
  resolves the stable-ABI functions it uses into a table of function pointers, initialises the
  interpreter, sets `sys.path` to the recorded search path, imports the modules, and releases the
  lock. Built with `Py_LIMITED_API` set to 3.10's value; it includes no Python header, declaring
  the few signatures it calls itself, so building a program needs no Python development files.
- **The LLVM emitter** compiles `CallPython` (`specs/shared-ir/`), which it refuses today: the
  arguments go through the runtime's converters, then a call to the bridge, then the result's
  conversion back into `T ! PyError`.
- **The driver** asks the found Python, once per build, for
  `sysconfig`'s shared library path (`LDLIBRARY` in `LIBDIR` on Unix, `python3XY.dll` beside
  `sys.base_prefix` on Windows), `sys.path`, `sys.version_info` and `Py_GIL_DISABLED` (R2.1,
  R2.4), and emits the first two as constant strings the bridge reads.

## Converting values

Serves R1.3, R1.4, R1.6.

Conversion is driven by the type descriptors the runtime already has: each descriptor gains a
shape (integer of a width and signedness, float, bool, str, unit, optional, list, tuple, dict,
set) so one pair of functions, `lt_py_from(desc, value)` and `lt_py_to(desc, object, out)`,
converts any value R1.4 lists. Going to Python copies; coming back checks as it copies — an
integer out of its type's range, or a value of the wrong type, is the `PyError` the Python
target's `lotml_rt.foreign` returns, with the same kind and message text, which the parity tests
compare. An exception is fetched, its type's `__name__` and `str()` taken, and cleared. Every
reference the bridge takes is released on every path; the test build counts objects created and
released, as it counts cells.

## The interpreter lock

Serves R1.5.

The bridge holds the lock only inside a call: `PyGILState_Ensure` before converting the
arguments, `PyGILState_Release` after converting the result. A task of `parallel` calling Python
waits for the lock while the others run native code.

## Risks

- A Windows interpreter found through the `py` launcher or the Microsoft Store has its DLL in
  places a plain search misses; the driver records the path the interpreter itself reports, never
  a guess, and R2.3's message names it.
- A virtual environment's packages come from the recorded `sys.path`, so an executable moved to a
  machine without that environment fails at R2.3, naming the module. Distribution with Python
  packages is a later decision.
- The text of a conversion error must match `lotml_rt`'s byte for byte for R1.3; the parity tests
  for the bridge call the same functions on both targets.
