---
status: accepted
---

# 0035 · A Python overload crosses as ordered signatures, the call choosing the first that fits

## Context

adr:0031-a-python-name-no-stub-types-crosses-as-an-opaque-python-value and
adr:0034-a-python-class-crosses-as-a-nominal-handle-with-its-declared-members leave out a function
or member a stub declares with `@overload`, listed in a comment. Measured on the binding coverage
corpus with the classes of adr:0034 bound, 50 public functions and 8 class members are left out
for that reason alone — `re.compile`, `re.search`, `os.listdir`, `os.getenv`, `shutil.copy`,
`yaml.load`, `math.prod` among them — about as many as generics leave out (57).

An overload is how a stub says one Python function returns a different type for different
arguments: `os.listdir(".")` is a `list[str]`, `os.listdir(b".")` a `list[bytes]`. Python runs one
function whatever the overloads say; they exist only for a type checker, which, by the typing
specification, gives a call the return type of the first overload, in declaration order, whose
parameters accept its arguments. LotML has no overloading: a program declares a name once (E0210).

## Decision

The owner accepted this on 2026-10-09. An overload set crosses whole, as the signatures it declares, in its order, and a call is checked
against the first of them whose parameters accept its arguments, as a Python type checker does.
`specs/python-overloads/` specifies it.

- **Declared in the interface by repetition.** In a Python interface, and only there, a function,
  constructor or method may be declared more than once; the declarations are its overloads, in
  order. A program still declares a name once (E0210): an overload is used, never declared.
- **Chosen at the call, from the arguments' types.** Each argument is checked once, without an
  expected type, and the call takes the first overload its arguments fit, by count, keyword and
  type. A value that is not a `PyObject` fitting a `PyObject` parameter is a weaker fit, taken only
  when no overload fits without one: the binder widens a type it cannot write to `PyObject`, so
  such a parameter accepts what the stub's own type does not. The call is then that signature's,
  and so are its result and its boundary descriptors. A
  call no overload accepts is an error listing them. Checking each argument once keeps a nest of
  overloaded calls linear, at the cost of an argument's expected type when overloads are tried.
- **Bound in order, up to the first that cannot be.** The binder writes each overload as it would
  write a function, and stops at the first it cannot bind (a coroutine, a parameter named by a
  LotML keyword): leaving out an overload in the middle would hand its calls to a later one with
  another return type. An overload whose parameters, as written, are an earlier one's is never
  chosen, and is left out; a name keeps at most 64, past the 43 of the largest set in the corpus
  (`numpy.matmul`), so a hostile stub cannot make each call try thousands. Each one left out is
  listed with its reason, as a function is.
- **Called, never passed.** An overloaded function has no single type, so naming it other than to
  call it is refused.

## Consequences

- `os.listdir`, `os.getenv`, `re.compile`, `shutil.copy` and the like are bound typed where their
  first overloads bind; the coverage report measures how many.
- A parameter the binder widens to `PyObject` (a union) still accepts, as the weaker fit, an
  argument no other overload takes, where the stub's own type may refuse it. Its result is then
  checked at the boundary as any is, and a value of another type is an `Err(PyError)`, never a
  crash; a later spec that types unions narrows it.
- A generic overload (`functools.reduce`, `os.getenv` with a default) is bound as a generic function
  is, its type variables written `PyObject`, until `specs/python-generics/` types them.
- Ruled out: **one merged signature**, each parameter and the result widened to `PyObject` where
  the overloads disagree, which needs no checker change but makes `os.listdir` return a `PyObject`,
  reachable and never typed; **one name per overload** (`listdir`, `listdir_2`), names no Python
  documentation uses, which a model writing LotML would have to guess; and **the first overload
  only**, silently wrong for every call a later overload describes.
