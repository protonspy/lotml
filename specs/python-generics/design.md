# Python generics — design

## What changes

Serves R1.1, R1.2, R1.3, R1.4, R1.5, R2.1, R2.2, R2.3, R3.1, R3.2, R4.1, R4.2, R4.3, R4.4, R5.1, R5.2, R6.1, R6.2.

The decision is adr:0036-a-python-type-variable-crosses-as-a-type-parameter-checked-where-a-value-crosses;
this is where each layer takes it. It builds on adr:0034 (classes as handles) and adr:0035
(overloads).

**Parser** (`lotml-syntax`). `ClassDef` gains `type_params`, read after the name as a function's
are (`class Pattern[AnyStr]:`); every exhaustive match on it takes the field.

**Binder** (`lotml-bind/src/binder.rs`). `type_vars` becomes a table of each type variable's kind
— plain, bounded, constrained with its types, `ParamSpec`, `TypeVarTuple` — read from the stub and,
for a name it imports from `typing`, `typing_extensions` or `_typeshed`, from the embedded
typeshed's stub of that module. `Cx` carries the table, the class's own parameters and each
stub class's parameter count. `lotml_type` writes a plain or bounded variable by its name, a
`ParamSpec` or `TypeVarTuple` as `PyObject`, and a stub class by its name with its arguments
(`Pattern[str]`, `PyObject` for any left unnamed). A function or member whose signature uses a
variable that is not its class's is written with type parameters (`fn nlargest[_T](…)`); one that
uses a constrained one is expanded into an overload per constraint before `overloads()` writes the
run, so the cut, the repeats and the limit of adr:0035 hold. `bindable_classes` stops refusing a
generic class: `class_block` writes its parameters in PEP 484's order (`Generic[…]` when present,
else first use in the bases), and `self` with its annotation when it has one.

**Interface reader** (`lotml-check/src/interface.rs`). `python_signature` accepts type parameters
and refuses a bound (E0221). `python_class` reads the class's parameters: its own type is
`Adt(py.m.C, [Param…])`, each method a `Method` whose `owner_params` are the class's, and the
constructor and each static method take the class's parameters as their own leading type
parameters, so a call infers them. `FnSig` gains `python: bool`, set here and by `py_constructor`.

**Checker** (`lotml-check`). `Program::lower` gives a Python class's name its arguments, checked
for count against the class's (E0202). `call_signature` already instantiates type parameters; on a
`python` signature it also reports a type argument the boundary does not carry (E0204: not
`carried`, and not a Python class's value), and records the call. The record generalizes
`Checked::py_overloads` into `Checked::py_calls`, by the call's span: the overload chosen and the
signature as instantiated, its types resolved when the body's are. `call_overloaded` and `takes`
instantiate each candidate's type parameters and its owner's, so a generic overload can be chosen,
and fit the receiver's type arguments against an annotated `self`. The Python method and attribute
paths pass the receiver's type arguments as `owner_args`, as an `impl` of a generic record does.

**Lowering and runtime**. `python_call` and `python_method` take the recorded instantiation, so
operands are coerced to, and the result descriptor built from, concrete types; a type left unknown
is described as `PyObject`. The descriptor of a generic class stays `["class", module, name]`: the
runtime checks the class and not its arguments, which Python erased.

**Coverage**. `bound_names` reads a name up to its `[` as well as its `(`. The label `overloads` is
the measurement before this spec; `generics` is the one after.

## Data

A generic class and a function over a constrained variable, as the binder writes them:

```
fn compile(pattern: str, flags: PyObject = 0) -> Pattern[str] ! PyError
fn compile(pattern: bytes, flags: PyObject = 0) -> Pattern[bytes] ! PyError
fn nlargest[_T](n: int, iterable: [_T], key: None = None) -> [_T] ! PyError

class Pattern[AnyStr]:
    pattern: AnyStr
    fn search(self: Pattern[str], string: str, pos: int = 0) -> Match[str]? ! PyError
    fn search(self: Pattern[bytes], string: bytes, pos: int = 0) -> Match[bytes]? ! PyError
```

## Risks

- A handle's type arguments are not checked when it crosses, so a `Pattern[str]` that holds a
  bytes pattern is caught only at its first member read, as an `Err(PyError)`.
- Reading `typing` and `_typeshed` on each bind adds their parse to the binder's cost; they are
  parsed once a process.
