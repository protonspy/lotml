---
status: accepted
---

# 0013 · C libraries through interfaces named `c.<library>`

## Context

R17 asks for a foreign function interface with C, and the [[colorless-concurrency]] page asks
that a call which blocks be handed to a thread of its own rather than stall the program.
adr:0012-python-interop-through-checked-boundaries-and-interface-files already gives lotml a way
to reach foreign code without new syntax in program files: an interface of bodyless signatures,
`bindings/<module>.lotmli`, found up the directory tree from the importing file. On the Python
target, `ctypes` loads a shared library and calls its symbols. It releases the interpreter
while a call runs, so a call that blocks holds up only the thread it runs on, and each task of
`parallel` has a thread of its own. The C target (phase 3) will link the library instead, so a
missing library or symbol is an error before the program runs, not a value a call returns.

## Decision

Reach a C library through an interface named `c.<library>`. For the C math library that is
`bindings/c.m.lotmli`, imported with `from c.m import cos`. Its signatures are written by hand,
because a C header is not something the compiler reads. They use only the types C and lotml
pass by value:

- the integers `i8` to `u64` (`int` is `i64`), `f32`, `f64` and `bool`, in either direction;
- `str` as a parameter only, passed as a NUL-terminated UTF-8 `const char*`;
- `None` as `void`.

A C function returns `T`, not `T ! E`, since C reports no exceptions. The library and each of
its symbols are loaded when the importing module loads. If either is missing, the program stops
there, as linking would. A blocking call runs on the calling task's own thread with the
interpreter released. On the Python target that already is a dedicated thread, so the rest of
the program goes on.

Rejected alternatives:
- An `extern` declaration in program files, which would be a second way to declare a function.
- Reading C headers. Macros and platform `#if`s make a header a program, not a list of types.
- Pointers, structs, arrays and callbacks. Each needs an ownership rule the value semantics do
  not have yet; they are recorded as ceilings.

## Consequences

- An interface's name now says which world it binds. `c.<library>` follows the C rules, and
  any other name follows Python's rules from adr:0012.
- A value crosses into C without a copy and comes back as a new value, since only scalars and
  read-only strings cross.
- Code that needs a C pointer goes through a Python module that wraps the library, at the
  cost of `T ! PyError` on each call.
