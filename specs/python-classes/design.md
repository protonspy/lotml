# Python classes — design

## What changes

Serves R1.1, R1.2, R1.3, R1.4, R1.5, R2.1, R2.2, R2.3, R2.4, R2.5, R3.1, R3.2, R3.3, R4.1.

The decision is adr:0034-a-python-class-crosses-as-a-nominal-handle-with-its-declared-members;
this is where each layer takes it.

**Parser** (`lotml-syntax`). In interface mode only (`parse_interface`, `parser.rs:36`), `class`
starts an `Item::Class(ClassDef)`: a name, the bases in parentheses, and an indented block of
attribute lines (`name: Type`) and bodyless `fn` signatures. A program keeps E0102 at `class`
(`parser.rs:395`). Every exhaustive match on `Item` treats a class as a program never holds one.

**Interface reader** (`lotml-check/src/interface.rs`). `interface()` reads `Item::Class` beside
`Item::Fn`: an `Interface` gains `classes`, each with its attributes, constructor, methods and
static methods as `FnSig`s, and the bases among the interface's own classes. The type names inside
an interface resolve to its classes first, so a function returning `date` is typed. A class in a C
interface, a base the interface does not declare, or a member without `PyError` is E0221.

**Checker**. A Python class is `Ty::Adt("py.<module>.<Class>", [])`: the dots keep it apart from
every LotML record, which a program names with an identifier, and the existing unification by name
holds. `Program` gains `classes` beside `available`; `import_python` (`program.rs:328`) makes an
imported class's name a type and its constructor a function: its own, else the nearest base's,
made where a program calls it and never kept for every subclass (`interface::py_constructor`), so
a module of many subclasses of one wide base costs what it is. The member hooks are those records
use: `attribute()` (`body.rs:2098`) gives `T ! PyError` for a declared attribute and refuses an
assignment; `method_call` (`body.rs:2754`) looks a method up through the class and its bases, and
a `Ty::TypeName` receiver (`body.rs:2762`) finds a static method. The coercion to a base is added
beside `PyObject <- T`. Everything else a value can do is refused as `py_value` (`body.rs:168`)
refuses it on a `PyObject`, and `holds_py_object` (`ty.rs:92`) counts a class, which brings the
refusals of printing, comparing and hashing a value holding one.

**Lowering and runtime**. The descriptor of a class is `["class", module, name]`
(`lotml-py/src/boundary.rs:15`). A constructor and a static method are the `CallPython` a function
is, `foreign` resolving a dotted path (`date.today`) attribute by attribute. A method call and an
attribute read are two new IR expressions, `CallPyMethod` and `PyAttribute`, emitted as
`rt.method(handle, name, descriptor, args)` and `rt.attribute(handle, name, descriptor)`, which
turn an exception into `Err(PyError)` as `foreign` does. `accept` checks a `["class", m, C]` value
with `isinstance` against `C` of the imported module `m` and wraps it in a `PyHandle`, which
`to_python` unwraps and `copy` shares (`lotml_rt.py:442`, `:618`).

**Native target**. With `holds_py_object` counting a class, E0402 refuses it where it refuses a
`PyObject` (`lotml-llvm/src/lib.rs:82`).

**Binder** (`lotml-bind/src/binder.rs`). `functions()` (`binder.rs:120`) gains the classes beside
the functions: a public `ClassDef` whose bases are not `Generic[...]` and that has no type
parameters; its `__init__`, or `__new__` without one, as the constructor, its `self` methods, its
`@staticmethod` and `@classmethod` ones, its annotated attributes and `@property`s. `lotml_type`
(`binder.rs:252`) writes a class of the same stub by its name. Each member it leaves out is a
`#   C.name: reason` comment.

**Coverage**. `binding_coverage.bound_names` counts a `class C` line of the interface as `C` bound
typed; `public_names` already counts a class. The label `embedded-typeshed` is the measurement
before this spec; `classes` is the one after.

## Data

A class in an interface, as the binder writes it:

```
class date:
    year: int
    month: int
    fn date(year: int, month: int, day: int) -> date ! PyError
    fn today() -> date ! PyError
    fn isoformat(self) -> str ! PyError
    fn replace(self, year: int = todo(), month: int = todo(), day: int = todo()) -> date ! PyError

class datetime(date):
    fn now() -> datetime ! PyError
```

## Risks

- A stub declares attributes on instances that some objects lack at run time; a read is
  `T ! PyError`, so such a read is an error value, never a crash.
- Bases outside the interface (`object`, a class imported from another module) are dropped, so a
  subclass of an imported class is not accepted where that class is, until re-exports bind it.
