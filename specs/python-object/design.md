# Python object — design

## The type

Serves R1.1, R1.2, R1.3.

`Ty::PyObject`, written `PyObject`, is a primitive of the checker as `str` is, read in programs and
in interfaces alike. It unifies only with itself. Every operation a value has is refused on it
with E0204 or E0205 and the note "a `PyObject` is opaque: take a LotML value out of it with
`o.value()`, its type annotated": attribute, method other than `value`, operator, comparison,
`in`, iteration, `len`, `print`, an f-string, a conversion such as `str(o)`. A `PyObject`
parameter of a Python function accepts a `PyObject` and any value whose type the boundary carries
(numbers, `bool`, `str`, `bytes`, `None`, lists, sets, dicts, tuples and optionals of them): the
coercion `PyObject <- T` is added beside the coercions to an optional and to a `dyn`.

## The conversion

Serves R2.1, R2.2, R2.3.

`o.value()` is checked against the type its context expects, as an empty `[]` is: a binding's
annotation, a parameter, a return type, a record field. Given `T`, it is `T ! PyError`; with no
expected type it is E0205, saying to annotate the binding. `int(o)` and `str(o)` are not
conversions: Python's `str(obj)` formats any object, so reading it as a checked conversion would
mislead, and both are refused with the same note.

Lowering writes it as the runtime's `convert(value, descriptor, types, classes)`, which runs
`accept`, the check of adr:0012 a returned value already goes through, and gives `Ok` of the copy
or `Err(PyError)` with the `TypeError` or `OverflowError` it raised.

## Crossing into Python

Serves R1.4, R1.5.

The runtime's `copy` already copies LotML's values part by part and leaves any other object as it
is; `foreign` calls it on the arguments in place of `deepcopy`, so a `PyObject` an argument holds,
alone or inside a list, reaches Python as the object it is. A LotML record is told from a Python
dataclass by a marker `record()` sets on the class, so a Python dataclass held in a `PyObject` is
never copied. A result declared `PyObject` has the descriptor `["any"]`, which `accept` passes
through.

## Where it stops

Serves R3.1, R3.2.

The LLVM target refuses a function whose types hold a `PyObject` with E0402, naming the Python
target, before it lays anything out; a program that imports Python is already refused at the
import. `boundary::exports` leaves out a function taking or returning a `PyObject`, and the compiled
module carries an E0403 warning for each, as the C library export does.

## The binder and the report

Serves R4.1, R4.2.

`lotml_bind.py` writes `PyObject` for a type it has no LotML type for: `Any`, a union other than
`X | None` (and `PyObject` for `X | None` when `X` is one), a class, a callable, a tuple of any
length, a missing annotation. An overloaded function and a coroutine stay unbound. The binding
coverage report counts a function whose signature holds `PyObject` as reachable, apart from typed.
