# Python overloads — design

## What changes

Serves R1.1, R1.2, R1.3, R1.4, R1.5, R2.1, R2.2, R3.1, R3.2, R3.3, R3.4, R4.1, R4.2.

The decision is adr:0035-a-python-overload-crosses-as-ordered-signatures-chosen-at-the-call; this
is where each layer takes it.

**Binder** (`lotml-bind/src/binder.rs`). An overload set is the run of `@overload` definitions of
one name in one statement list, the first such run of the name winning, as the first definition of
a function wins today. `functions()` and `class_block()` write each overload of the run with
`written()` / the member line they already use, and stop at the first that fails or whose receiver
differs from the first's; a written parameter list equal to an earlier one's is left out as never
chosen, and a run past 64 is cut there. Each one left out is a `#   name: reason` line.

**Interface reader** (`lotml-check/src/interface.rs`). `FnSig` gains `overloads: Vec<FnSig>`, the
declarations after the first, in order, so every map that holds a signature — the module's
functions, a class's constructor and methods — carries its overloads with no new map beside it.
In a Python interface a repeated `fn` (or a repeated member of a class block) is pushed onto the
first's `overloads` rather than reported E0210; a C interface and a program are unchanged. A member
overload whose receiver differs from the first's, and one past 64, is E0221. `py_constructor`
rewrites the result of every overload of an inherited constructor, not the first alone.

**Checker** (`lotml-check/src/body.rs`). `call_signature` is where every function, constructor,
static and method call is checked; on a signature with overloads it hands the call to a new
`call_overloaded`, which checks each argument once with no expected type, then tries the overloads
in order: the arguments fit by count and keyword as `call_signature` places them, and by type with
`fits` under a snapshot of the inference variables that is rolled back after each trial. A trial
that passes a value other than a `PyObject` to a `PyObject` parameter is remembered and taken only
when no overload fits without one. The chosen overload's types are then coerced as `call_signature`
coerces, and its index is recorded by the call's span (`Checked::py_overloads`, carried through
`PartChecked` and shifted as `locals` are). No overload fitting is E0204 with a note per overload.
`name()` gives an overloaded function's name no value and reports E0226.

**Lowering** (`lotml-ir/src/lower.rs`). `python_call` and `python_method` take the signature through
`checked.py_overload(span, sig)`, which returns the overload the checker recorded for the call, so
the `CallPython` and `CallPyMethod` operands and descriptors are that overload's. The runtime is
unchanged: Python runs one function whatever the overloads say.

**Coverage** (`harness/lotml_harness/experiments/binding_coverage.py`). `bound_names` sees an
overloaded name once per `fn` line; it is typed when any of its lines is. The label `classes` is
the measurement before this spec; `overloads` is the one after.

## Data

An overloaded function, as the binder writes it:

```
fn listdir(path: str? = todo()) -> list[str] ! PyError
fn listdir(path: PyObject) -> list[bytes] ! PyError
fn listdir(path: int) -> list[str] ! PyError
```

`listdir(".")` takes the first, `listdir(3)` the third: the second takes `3` only through its
`PyObject` parameter, a weaker fit. `listdir(b".")` fits only the second.

## Risks

- A parameter widened to `PyObject` is still the first fit for an argument no other overload
  takes, even where the stub's own type would refuse it; the result is checked at the boundary,
  so a mismatch is an `Err(PyError)`, never a crash.
- An argument is checked with no expected type when overloads are tried, so an integer literal
  given to an `i32` parameter of an overload is an `int` there and does not fit it.
