---
status: accepted
---

# 0036 · A Python type variable crosses as a type parameter, checked where a value crosses

## Context

adr:0031-a-python-name-no-stub-types-crosses-as-an-opaque-python-value binds a stub's type variable
as `PyObject`. adr:0034-a-python-class-crosses-as-a-nominal-handle-with-its-declared-members
leaves out a generic class, and adr:0035-a-python-overload-crosses-as-ordered-signatures-chosen-at-the-call
binds a generic overload with its type variables as `PyObject`. That leaves two parts of the binding
coverage corpus opaque or out:

- 57 generic classes are left out: `re.Pattern` and `Match`, `collections.deque`, `Counter` and
  `defaultdict`, the `itertools` iterators, `csv.DictReader`, `numpy.ndarray`, and the
  `ParseResult` that `urllib.parse.urlparse` returns.
- About 50 public functions that use a type variable cross as `PyObject`: `heapq.nlargest`,
  `statistics.mean`, `functools.reduce`, `math.prod`.

Their type variables come in three kinds: 31 plain (`T = TypeVar("T")`), 16 with a bound
(`bound=SupportsRichComparison`) and 8 constrained to a list of types
(`AnyStr = TypeVar("AnyStr", str, bytes)`).

LotML has generics already: `fn first[T](xs: [T]) -> T`, records `type Box[T](item: T)`, each
call's type arguments inferred, and bounds that name a LotML trait. An interface refuses type
parameters today (E0221), because nothing checks what Python hands back for a `T`.

## Decision

The owner accepted this on 2026-10-09. A Python type variable becomes a type parameter of the LotML
signature or class that uses it, inferred at each call as LotML's own are, and checked at the
boundary as the type it was inferred as. `specs/python-generics/` specifies it.

- **A plain type variable is a type parameter.** `def nlargest(n: int, iterable: Iterable[T]) ->
  list[T]` is bound as `fn nlargest[T](n: int, iterable: [T]) -> [T] ! PyError`. A call infers
  `T`. A value of a type the boundary does not carry, such as a LotML record, may not be given for
  one, as it may not be given for a `PyObject`.
- **A constrained type variable is an overload per constraint.** `AnyStr` is `str` or `bytes`,
  which is what an overload set says, so `def compile(pattern: AnyStr) -> Pattern[AnyStr]` is
  bound as two overloads, one for each type. This reuses adr:0035. Several constrained variables
  in one signature multiply, within the 64 overloads a name keeps.
- **A bound is not written.** LotML has no trait for Python's protocols
  (`SupportsRichComparison`), and Python checks the bound when it runs the call, so an argument
  outside it is an `Err(PyError)` like any other failure in Python. Writing the bound as a trait
  would refuse calls Python accepts.
- **A generic class is a handle with type arguments.** `class Pattern(Generic[AnyStr])` is written
  in the interface as `class Pattern[AnyStr]:`, and a value of it is `Pattern[str]`. Its members'
  types are substituted, as an `impl` of a generic record's are. A method overload restricted to
  one instantiation (`def search(self: Pattern[str], …)`) is chosen by the receiver's type
  arguments too. Type arguments are invariant, as LotML's are.
- **Checked where a value crosses, not on the handle.** At run time Python erases type arguments,
  so a handle declared `Pattern[str]` cannot be checked to be one. What is checked is every value
  read through the handle (a method's result, an attribute) against the type its instantiation
  says, so a mismatch is an `Err(PyError)`, never a value of the wrong type in the program.
- **Left as they are.** `ParamSpec`, `TypeVarTuple`, a callable and a generic protocol stay
  `PyObject` or out, each listed with its reason.

## Consequences

- A program can hold a compiled pattern and its matches typed: `p: Pattern[str] =
  compile(r"\d+")?`, then `m = p.search(text)?`, an optional `Match[str]`. It can also call the
  `heapq`, `statistics` and `itertools` functions on its own lists, with typed results.
- The handle's type arguments are a claim nothing checks until a value crosses. This is the
  price of erasure, and it is paid at the boundary already in place.
- The descriptor for a call's result is built from the types the checker inferred for that call,
  so the lowering reads the instantiation the checker recorded rather than the signature alone.
- Ruled out:
  - **Keeping `PyObject`**: typed nothing, which is the gap this closes.
  - **Mapping bounds to LotML traits**: no LotML trait has the meaning of a Python protocol, and
    it would refuse calls Python accepts.
  - **Checking a handle's type arguments by sampling its contents**: costs a walk per crossing,
    and is wrong for any container that is empty or that changes.
