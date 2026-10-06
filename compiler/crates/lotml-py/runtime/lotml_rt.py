"""The runtime of Python modules compiled from lotml.

A compiled module is a stub that hands `load` the program as a Python syntax tree whose nodes
carry the lotml positions, so a traceback names the `.lotml` file, shows its line and underlines
the expression. The rest is what that tree calls: results and failures, panics, the overflow
trap, value semantics, records and variants, the prelude and the built-in methods whose lotml
behaviour differs from Python's.
"""

import _string
import ast
import builtins
import copy as _copy
import dataclasses
import heapq
import json
import linecache
import math as _math
import string
import types
from typing import Any

I64_MIN, I64_MAX = -(2**63), 2**63 - 1

# Loading -----------------------------------------------------------------------------------


def _node(value):
    """A syntax tree node from its JSON form: `{"_": class, field: value, …}`."""
    if isinstance(value, list):
        return [_node(v) for v in value]
    if not isinstance(value, dict):
        return value
    if "_int" in value:
        return int(value["_int"], 0)
    if "_float" in value:
        return float(value["_float"].replace("_", ""))
    if "_bytes" in value:
        return bytes(value["_bytes"])
    cls = getattr(ast, value["_"])
    fields = {k: _node(v) for k, v in value.items() if k in cls._fields}
    node = cls(**fields)
    for attribute in ("lineno", "col_offset", "end_lineno", "end_col_offset"):
        if attribute in value:
            setattr(node, attribute, value[attribute])
    return node


def load(namespace: dict, path: str, payload: str, prelude: dict | None = None):
    """Run a compiled lotml program in `namespace`, the stub module's globals, and return its
    code. A host may give its own `prelude`: a `print` that stops at an output limit, say."""
    program = json.loads(payload)
    source = program["source"]
    linecache.cache[path] = (len(source), None, source.splitlines(keepends=True), path)
    module = ast.fix_missing_locations(_node(program["module"]))
    code = compile(module, path, "exec")
    namespace.update(
        {
            "__builtins__": PRELUDE if prelude is None else prelude,
            "__rt": _this(),
            "__variants": Variants(),
            "__tests": [],
            "__lotml__": path,
        }
    )
    exec(code, namespace)  # noqa: S102 - the program the compiler checked
    return code


def stub_arguments(stub: str) -> tuple[str, str]:
    """The path and the payload a compiled module's stub hands `load_module` (or, from an older
    compiler, `load`)."""
    tree = ast.parse(stub)
    for node in ast.walk(tree):
        if (
            isinstance(node, ast.Call)
            and isinstance(node.func, ast.Attribute)
            and node.func.attr in ("load", "load_module")
        ):
            return ast.literal_eval(node.args[1]), ast.literal_eval(node.args[2])
    raise ValueError("not a module compiled by lotml")


def _this():
    import sys

    return sys.modules[__name__]


# Results and failures ---------------------------------------------------------------------


@dataclasses.dataclass(frozen=True)
class Ok:
    value: Any


@dataclasses.dataclass(frozen=True)
class Err:
    error: Any


class Fail(Exception):
    """Carries an error from `fail` or `?` to the fallible function that returns it."""

    def __init__(self, error):
        super().__init__(error)
        self.error = error


def fail(error):
    raise Fail(error)


def unwrap(result):
    """`expr?`: the value of an `Ok`, or the error of an `Err` failing the caller."""
    if isinstance(result, Ok):
        return result.value
    raise Fail(result.error)


def coalesce(value, default):
    """`x ?? d`, with `d` a thunk so that `x ?? fail e` fails only on None."""
    return value if value is not None else default()


# Panics: the program stops ----------------------------------------------------------------


class Panic(Exception):
    """A broken invariant: the program stops."""


class Overflow(Panic):
    """An `int` result outside i64: lotml traps in every build (R26)."""


class Todo(Panic):
    """`todo()` reached at run time."""


class NonExhaustiveMatch(Panic):
    """No arm matched."""


def overflow(value):
    raise Overflow(f"{value} does not fit in int")


def no_match(subject):
    raise NonExhaustiveMatch(repr(subject))


def todo(*_args):
    raise Todo("not written yet")


def forbidden(name):
    """An attribute the checker should have refused — a `__` name, or a member of a value it left
    untyped; reaching it at run time is a bug."""
    raise Panic(f"the attribute `{name}` is not reachable from lotml")


def i64(value):
    """`value`, an `int`, checked against i64."""
    if not I64_MIN <= value <= I64_MAX:
        overflow(value)
    return value


def check(value, low, high):
    """`value`, an integer result, checked against its type's range where `:=` cannot go."""
    if not low <= value <= high:
        overflow(value)
    return value


def power(base, exponent):
    """`base ** exponent`: an `int` result too wide for i64 traps before it is computed."""
    if type(base) is int and type(exponent) is int:
        if exponent < 0:
            raise ValueError("a negative exponent of an int")
        if abs(base) > 1 and (abs(base).bit_length() - 1) * exponent > 64:
            overflow(f"{base} ** {exponent}")
        return i64(base**exponent)
    return base**exponent


def lshift(value, amount):
    """`value << amount`: a shift leaving i64 traps before it is computed."""
    if value != 0 and amount >= 64:
        overflow(f"{value} << {amount}")
    return i64(value << amount)


def _wrap(value: int) -> int:
    return (value - I64_MIN) % 2**64 + I64_MIN


def wrapping_add(a: int, b: int) -> int:
    return _wrap(a + b)


def wrapping_sub(a: int, b: int) -> int:
    return _wrap(a - b)


def wrapping_mul(a: int, b: int) -> int:
    return _wrap(a * b)


# Value semantics ---------------------------------------------------------------------------


def copy(obj):
    """A deep copy: a value entering a `var`, or leaving one, is its own."""
    return _copy.deepcopy(obj)


def shallow(obj):
    """A copy of a collection whose elements are immutable."""
    return _copy.copy(obj)


class Box:
    """An `inout` argument: the callee reads and writes `value`, the caller takes it back."""

    __slots__ = ("value",)

    def __init__(self, value):
        self.value = value


def returning(result, *_writebacks):
    """The value of a call whose `inout` arguments were written back after it."""
    return result


def set_attr(obj, name: str, value):
    setattr(obj, name, value)


def set_item(obj, key, value):
    obj[key] = value


# Records and variants ----------------------------------------------------------------------


class Unit:
    """A variant without fields: one instance, equal only to itself, never copied."""

    __match_args__ = ()

    def __init__(self, name: str):
        self.name = name

    def __repr__(self):
        return self.name

    def __deepcopy__(self, memo):
        return self

    def __copy__(self):
        return self

    def __lt__(self, other):
        return isinstance(other, Unit) and self.name < other.name


class Variants:
    """The namespace value patterns refer to: `case Empty` matches `__variants.Empty`."""


def record(name: str, fields: tuple[str, ...], defaults: dict):
    """A record type, or a variant with fields. `defaults` maps a field to a thunk, so no
    default value is shared between two records."""
    spec = [
        (f, Any, dataclasses.field(default_factory=defaults[f])) if f in defaults else (f, Any)
        for f in fields
    ]
    cls = dataclasses.make_dataclass(name, spec, eq=True, order=True)
    cls.__hash__ = lambda self: hash(tuple(_hashable(getattr(self, f)) for f in fields))
    return cls


def _hashable(obj):
    if isinstance(obj, list):
        return tuple(_hashable(v) for v in obj)
    if isinstance(obj, dict):
        return tuple(sorted((k, _hashable(v)) for k, v in obj.items()))
    if isinstance(obj, set):
        return frozenset(obj)
    return obj


def attach(target, name: str, function, is_method: bool):
    setattr(target, name, function if is_method else staticmethod(function))


def attach_defaults(target, defaults: dict):
    """A trait's default methods, for each one the `impl` did not define."""
    for name, function in defaults.items():
        if not hasattr(target, name):
            setattr(target, name, function)


# The boundary with Python ------------------------------------------------------------------


@dataclasses.dataclass(frozen=True)
class PyError:
    """What a call into Python raised, as a lotml error: its exception class and message."""

    kind: str
    message: str


def python(function, *args, **kwargs):
    """Call into Python: every such call can raise, so its result is `T ! PyError`."""
    try:
        return Ok(function(*args, **kwargs))
    except Exception as error:  # noqa: BLE001 - the boundary turns every exception into a value
        return Err(PyError(type(error).__name__, str(error)))


TASK_THREADS = 256
"""The most tasks of one `parallel` that run at once; the rest wait for a thread."""


def parallel(tasks):
    """`parallel([lambda: …, …])` (R20): each task on a thread of its own, so a call that blocks
    — into Python, say — holds up only its task; the caller waits for them all and gets their
    results in order. Nothing is marked `async`. A task reaches no value another can change: a
    lambda captures copies, and every other value is immutable. A task that panicked stops the
    program once the others have finished."""
    import concurrent.futures

    tasks = list(tasks)
    if not tasks:
        return []
    workers = min(len(tasks), TASK_THREADS)
    with concurrent.futures.ThreadPoolExecutor(workers, thread_name_prefix="lotml-task") as pool:
        futures = [pool.submit(task) for task in tasks]
        concurrent.futures.wait(futures)
    for future in futures:
        error = future.exception()
        if error is not None:
            raise error
    return [future.result() for future in futures]


class LotmlError(Exception):
    """What a lotml function returning `T ! E` raises when Python calls it and it fails: the
    lotml error is in `error`."""

    def __init__(self, error):
        super().__init__(error)
        self.error = error


def accept(value, descriptor, types: dict, classes: dict, where: str):
    """`value` as the lotml type `descriptor` describes, copied, so the side that gave it never
    shares it; a `TypeError` or `OverflowError` saying where, when it is not one.

    A descriptor is a list: `["int", low, high, name]`, `["float"]`, `["bool"]`, `["str"]`,
    `["bytes"]`, `["none"]`, `["list", d]`, `["set", d]`, `["dict", k, v]`, `["tuple", d…]`,
    `["optional", d]`, `["adt", name]` — a record or sum type described in `types` and built
    from `classes` — or `["any"]`."""
    kind = descriptor[0]

    def wrong(expected: str):
        return TypeError(f"{where}: expected {expected}, got {type(value).__name__}")

    if kind == "any":
        return _copy.deepcopy(value)
    if kind == "int":
        if isinstance(value, bool) or not isinstance(value, int):
            raise wrong(descriptor[3])
        if not descriptor[1] <= value <= descriptor[2]:
            raise OverflowError(f"{where}: {value} does not fit in {descriptor[3]}")
        return int(value)
    if kind == "float":
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            raise wrong("float")
        return float(value)
    if kind == "bool":
        if not isinstance(value, bool):
            raise wrong("bool")
        return value
    if kind == "str":
        if not isinstance(value, str):
            raise wrong("str")
        return str(value)
    if kind == "bytes":
        if not isinstance(value, (bytes, bytearray)):
            raise wrong("bytes")
        return bytes(value)
    if kind == "none":
        if value is not None:
            raise wrong("None")
        return None
    if kind == "optional":
        return None if value is None else accept(value, descriptor[1], types, classes, where)
    if kind == "list":
        if not isinstance(value, list):
            raise wrong("list")
        return [
            accept(v, descriptor[1], types, classes, f"{where}[{i}]") for i, v in enumerate(value)
        ]
    if kind == "set":
        if not isinstance(value, (set, frozenset)):
            raise wrong("set")
        return {accept(v, descriptor[1], types, classes, f"{where} element") for v in value}
    if kind == "dict":
        if not isinstance(value, dict):
            raise wrong("dict")
        return {
            accept(k, descriptor[1], types, classes, f"{where} key"): accept(
                v, descriptor[2], types, classes, f"{where}[{k!r}]"
            )
            for k, v in value.items()
        }
    if kind == "tuple":
        items = descriptor[1:]
        if not isinstance(value, tuple) or len(value) != len(items):
            raise wrong(f"a tuple of {len(items)}")
        return tuple(
            accept(v, d, types, classes, f"{where}[{i}]")
            for i, (v, d) in enumerate(zip(value, items, strict=True))
        )
    if kind == "adt":
        return _accept_adt(value, descriptor[1], types, classes, where)
    raise TypeError(f"{where}: no lotml type `{kind}`")


def _accept_adt(value, name: str, types: dict, classes: dict, where: str):
    """A record of type `name`, or a variant of the sum type `name`, rebuilt field by field."""
    shape = types.get(name)
    if shape is None:
        return _copy.deepcopy(value)
    cases = [(name, shape[1])] if shape[0] == "record" else shape[1]
    for case, fields in cases:
        cls = classes.get(case)
        if fields is None:
            if value is cls:
                return value
            continue
        if isinstance(cls, type) and type(value) is cls:
            built = {
                field: accept(getattr(value, field), d, types, classes, f"{where}.{field}")
                for field, d in fields
            }
            return cls(**built)
    raise TypeError(f"{where}: expected {name}, got {type(value).__name__}")


def export(function, name: str, spec: dict, types: dict, classes: dict):
    """A lotml function as Python calls it: its arguments checked against the lotml signature
    and copied, and a failure raised as `LotmlError` rather than returned."""
    params = spec["params"]
    names = [p[0] for p in params]

    def exported(*args, **kwargs):
        if spec.get("inout"):
            raise TypeError(f"{name} changes an argument in place (`inout`): call it from lotml")
        if len(args) > len(params):
            raise TypeError(f"{name}() takes {len(params)} arguments, {len(args)} were given")
        given = dict(zip(names, args, strict=False))
        for key, value in kwargs.items():
            if key not in names:
                raise TypeError(f"{name}() has no parameter `{key}`")
            if key in given:
                raise TypeError(f"{name}() was given `{key}` twice")
            given[key] = value
        missing = [p[0] for p in params if p[0] not in given and not p[2]]
        if missing:
            raise TypeError(f"{name}() is missing {', '.join(missing)}")
        checked = {
            p[0]: accept(given[p[0]], p[1], types, classes, f"{name}({p[0]})")
            for p in params
            if p[0] in given
        }
        result = function(**checked)
        if spec["error"] is None:
            return result
        if isinstance(result, Err):
            raise LotmlError(result.error)
        return result.value

    exported.__name__ = exported.__qualname__ = name
    exported.__doc__ = getattr(function, "__doc__", None)
    exported.__wrapped__ = function
    return exported


def load_module(namespace: dict, path: str, payload: str):
    """Load a compiled module as Python imports it: the program runs in a namespace of its own,
    and the module shows its types as they are and its functions behind the checked boundary
    of `export` (adr:0012)."""
    program = {"__name__": namespace.get("__name__", "lotml_program")}
    code = load(program, path, payload)
    exports = json.loads(payload).get("exports", {"functions": {}, "types": {}, "names": []})
    for name in exports["names"]:
        namespace[name] = program[name]
    for name, spec in exports["functions"].items():
        namespace[name] = export(program[name], name, spec, exports["types"], program)
    namespace.update({"__program": program, "__tests": program["__tests"], "__lotml__": path})
    return code


def foreign(module: str, name: str, returns: str):
    """A Python function a lotml program calls through its interface: the arguments copied, any
    exception and any returned value of the wrong type an `Err(PyError)`, the rest `Ok`."""
    descriptor = json.loads(returns)

    def call(*args, **kwargs):
        try:
            import importlib

            function = getattr(importlib.import_module(module), name)
            value = function(*_copy.deepcopy(args), **_copy.deepcopy(kwargs))
        except Exception as error:  # noqa: BLE001 - the boundary turns every exception into a value
            return Err(PyError(type(error).__name__, str(error)))
        try:
            return Ok(accept(value, descriptor, {}, {}, f"{module}.{name}() returned"))
        except (TypeError, OverflowError) as error:
            return Err(PyError(type(error).__name__, str(error)))

    call.__name__ = call.__qualname__ = name
    return call


_C_LIBRARIES: dict = {}


def _c_library(name: str):
    """A C library by its name — `m`, `c`, `msvcrt` — as the platform finds it."""
    import ctypes
    import ctypes.util

    if name not in _C_LIBRARIES:
        found = ctypes.util.find_library(name)
        try:
            _C_LIBRARIES[name] = ctypes.CDLL(found or name)
        except OSError as error:
            raise LinkError(f"the C library `{name}` cannot be loaded: {error}") from None
    return _C_LIBRARIES[name]


class LinkError(Panic):
    """A C library or one of its functions that is not there: the program cannot start."""


def c_function(library: str, name: str, params: list, returns: str):
    """A C function a lotml program calls through its interface (adr:0013), loaded when the
    module loads. `ctypes` releases the interpreter while the call runs, so a call that blocks
    holds up only its own task's thread."""
    import ctypes

    kinds = {
        "i8": ctypes.c_int8,
        "i16": ctypes.c_int16,
        "i32": ctypes.c_int32,
        "i64": ctypes.c_int64,
        "u8": ctypes.c_uint8,
        "u16": ctypes.c_uint16,
        "u32": ctypes.c_uint32,
        "u64": ctypes.c_uint64,
        "f32": ctypes.c_float,
        "f64": ctypes.c_double,
        "bool": ctypes.c_bool,
        "str": ctypes.c_char_p,
        "none": None,
    }
    try:
        function = getattr(_c_library(library), name)
    except AttributeError:
        raise LinkError(f"the C library `{library}` has no function `{name}`") from None
    function.argtypes = [kinds[p] for p in params]
    function.restype = kinds[returns]

    def call(*args):
        converted = []
        for value, param in zip(args, params, strict=True):
            if param == "str":
                if "\0" in value:
                    raise Panic(f"{name}: a str holding a NUL byte cannot be passed to C")
                value = value.encode("utf-8")
            converted.append(value)
        return function(*converted)

    call.__name__ = call.__qualname__ = name
    return call


class ForeignModule:
    """`import m` of a Python module with an interface: its functions, through `foreign`."""

    def __init__(self, module: str, functions: dict):
        for name, returns in functions.items():
            setattr(self, name, foreign(module, name, returns))


# The prelude -------------------------------------------------------------------------------


class Heap:
    """`Heap[T]`: a min-heap."""

    def __init__(self, items=()):
        self.items = list(items)
        heapq.heapify(self.items)

    def push(self, item):
        heapq.heappush(self.items, item)

    def pop_min(self):
        return heapq.heappop(self.items) if self.items else None

    def peek(self):
        return self.items[0] if self.items else None

    def __len__(self):
        return len(self.items)

    def __eq__(self, other):
        return isinstance(other, Heap) and sorted(self.items) == sorted(other.items)

    __hash__ = None


def _sum(items, start=0):
    total = builtins.sum(items, start)
    return i64(total) if type(total) is int else total


def _abs(value):
    result = builtins.abs(value)
    return i64(result) if type(result) is int else result


def _int(value):
    return i64(builtins.int(value))


def _sized(name: str, low: int, high: int):
    """`i32(x)` and the rest: a number made the sized integer, stopping when it does not fit."""

    def convert(value):
        return check(builtins.int(value), low, high)

    convert.__name__ = convert.__qualname__ = name
    return convert


SIZED = {
    name: _sized(name, low, high)
    for name, low, high in (
        ("i8", -(2**7), 2**7 - 1),
        ("i16", -(2**15), 2**15 - 1),
        ("i32", -(2**31), 2**31 - 1),
        ("i64", I64_MIN, I64_MAX),
        ("u8", 0, 2**8 - 1),
        ("u16", 0, 2**16 - 1),
        ("u32", 0, 2**32 - 1),
        ("u64", 0, 2**64 - 1),
    )
}


def _round(value, digits=None):
    if digits is None:
        return i64(builtins.round(value))
    # `round(x, digits)` is a float in lotml, an int's too.
    return builtins.float(builtins.round(value, digits))


def _pow(base, exponent, modulus=None):
    if modulus is not None:
        return builtins.pow(base, exponent, modulus)
    return power(base, exponent)


def _divmod(a, b):
    quotient, remainder = builtins.divmod(a, b)
    return (i64(quotient) if type(quotient) is int else quotient), remainder


def _listed(function):
    def listed(*args, **kwargs):
        return list(function(*args, **kwargs))

    listed.__name__ = function.__name__
    return listed


def _checked(function):
    """A `math` function whose `int` result must fit in i64."""

    def checked(*args):
        result = function(*args)
        return i64(result) if type(result) is int else result

    checked.__name__ = function.__name__
    return checked


math = types.SimpleNamespace(
    **{
        name: getattr(_math, name)
        for name in (
            "sqrt",
            "pow",
            "log",
            "log2",
            "log10",
            "exp",
            "sin",
            "cos",
            "tan",
            "atan",
            "atan2",
            "hypot",
            "fabs",
            "pi",
            "e",
            "inf",
        )
    },
    **{
        name: _checked(getattr(_math, name))
        for name in ("floor", "ceil", "trunc", "isqrt", "factorial", "comb", "perm", "gcd")
    },
)
"""The `math` module as lotml sees it: an `int` result too wide for i64 traps."""

PRELUDE = {
    **{
        name: getattr(builtins, name)
        for name in (
            "print",
            "len",
            "range",
            "enumerate",
            "sorted",
            "min",
            "max",
            "any",
            "all",
            "float",
            "str",
            "bool",
            "ord",
            "chr",
            "set",
            "list",
            "dict",
            "hash",
        )
    },
    "sum": _sum,
    "abs": _abs,
    "int": _int,
    "round": _round,
    "pow": _pow,
    "divmod": _divmod,
    "reversed": _listed(reversed),
    "zip": _listed(zip),
    "map": _listed(map),
    "filter": _listed(filter),
    "Heap": Heap,
    "todo": todo,
    "Ok": Ok,
    "Err": Err,
    "PyError": PyError,
    "wrapping_add": wrapping_add,
    "wrapping_sub": wrapping_sub,
    "wrapping_mul": wrapping_mul,
    "isqrt": math.isqrt,
    "gcd": math.gcd,
    "parallel": parallel,
    **SIZED,
    "f32": builtins.float,
}
"""The names every program sees without an import (R35)."""

# Built-in methods whose lotml behaviour differs from Python's ------------------------------


def to_int(text: str):
    try:
        return _int(text.strip())
    except ValueError:
        return None


def to_float(text: str):
    try:
        return float(text.strip())
    except ValueError:
        return None


def str_find(text: str, sub: str, *bounds):
    at = text.find(sub, *bounds)
    return None if at < 0 else at


def str_rfind(text: str, sub: str, *bounds):
    at = text.rfind(sub, *bounds)
    return None if at < 0 else at


def split_once(text: str, sep: str):
    head, found, tail = text.partition(sep)
    return (head, tail) if found else None


def _format_fields(text: str) -> list[str]:
    fields = []
    for _literal, name, spec, _conversion in string.Formatter().parse(text):
        if name is not None:
            fields.append(name)
        if spec:
            fields += _format_fields(spec)
    return fields


def str_format(text: str, *args, **kwargs):
    """`str.format`, refusing a field that reads an attribute: `{0.x}` happens inside Python, out
    of the checker's sight, and walks the interpreter's objects whatever the name. A field may
    still name an argument and index into it: `{0}`, `{name}`, `{0[1]}`."""
    for field in _format_fields(text):
        _, rest = _string.formatter_field_name_split(field)
        if any(is_attribute for is_attribute, _ in rest):
            raise ValueError(f"the format field `{{{field}}}` reads an attribute; lotml does not")
    return text.format(*args, **kwargs)


def list_pop(items: list, *index):
    if not items:
        return None
    return items.pop(*index)


def list_last(items: list):
    return items[-1] if items else None


def list_find(items: list, predicate):
    return next((item for item in items if predicate(item)), None)


def list_index(items: list, item):
    try:
        return items.index(item)
    except ValueError:
        return None


def contains(collection, item) -> bool:
    return item in collection


def dict_pop(mapping: dict, key, *default):
    return mapping.pop(key, *default) if default else mapping.pop(key, None)


def keys(mapping: dict) -> list:
    return list(mapping.keys())


def values(mapping: dict) -> list:
    return list(mapping.values())


def items(mapping: dict) -> list:
    return list(mapping.items())


def set_pop(items: set):
    return items.pop() if items else None


# Tests and runs ----------------------------------------------------------------------------


class TestFailure(AssertionError):
    """A failed `assert`, with what each side of its comparison was."""

    def __init__(self, expression, op=None, left=None, right=None, message=None):
        super().__init__(expression)
        self.expression, self.op, self.left, self.right = expression, op, left, right
        self.message = message


def assertion_failed(expression, op, left, right, message=None):
    raise TestFailure(expression, op, left, right, message)


def show(value) -> str:
    """A value as lotml writes it."""
    if value is None or isinstance(value, bool | int):
        return repr(value)
    if isinstance(value, float):
        return repr(value)
    if isinstance(value, str):
        return json.dumps(value, ensure_ascii=False)
    if isinstance(value, list):
        return "[" + ", ".join(show(v) for v in value) + "]"
    if isinstance(value, tuple):
        inner = ", ".join(show(v) for v in value)
        return f"({inner},)" if len(value) == 1 else f"({inner})"
    if isinstance(value, set | frozenset):
        return "{" + ", ".join(sorted(show(v) for v in value)) + "}" if value else "set()"
    if isinstance(value, dict):
        return "{" + ", ".join(f"{show(k)}: {show(v)}" for k, v in value.items()) + "}"
    if isinstance(value, Ok):
        return f"Ok({show(value.value)})"
    if isinstance(value, Err):
        return f"Err({show(value.error)})"
    if isinstance(value, Heap):
        return f"Heap({show(sorted(value.items))})"
    if dataclasses.is_dataclass(value):
        fields = [f.name for f in dataclasses.fields(value)]
        if all(f.startswith("_") and f[1:].isdigit() for f in fields):
            parts = [show(getattr(value, f)) for f in fields]
        else:
            parts = [f"{f}={show(getattr(value, f))}" for f in fields]
        return f"{type(value).__name__}({', '.join(parts)})"
    return repr(value)


def _frames(error: BaseException, path: str) -> list[dict]:
    """The lotml frames of a traceback, outermost first."""
    frames = []
    trace = error.__traceback__
    while trace is not None:
        code = trace.tb_frame.f_code
        if code.co_filename == path:
            function = code.co_name
            frames.append({"function": function, "line": trace.tb_lineno})
        trace = trace.tb_next
    return frames


def run_tests(namespace: dict, path: str) -> list[dict]:
    """Run every `test` block: pass, fail with the values compared, an error passed on by `?`,
    or a panic with where it happened."""
    results = []
    for name, test in namespace["__tests"]:
        result = {"name": name}
        try:
            test()
            result["outcome"] = "pass"
        except TestFailure as failure:
            result["outcome"] = "fail"
            result["expression"] = failure.expression
            if failure.op is not None:
                result.update(op=failure.op, left=show(failure.left), right=show(failure.right))
            if failure.message is not None:
                result["message"] = show(failure.message)
            failure_frames = _frames(failure, path)
            result["line"] = failure_frames[-1]["line"] if failure_frames else None
        except Fail as failure:
            frames = _frames(failure, path)
            result.update(
                outcome="error",
                error=show(failure.error),
                line=frames[-1]["line"] if frames else None,
            )
        except Exception as error:  # noqa: BLE001 - a test reports every panic
            frames = _frames(error, path)
            result.update(
                outcome="panic",
                kind=type(error).__name__,
                message=str(error),
                line=frames[-1]["line"] if frames else None,
                trace=frames,
            )
        results.append(result)
    return results


def main(module_name: str) -> int:
    """`lotml run`: call the program's `main`, and say what stopped it, in lotml's terms."""
    import importlib
    import sys
    import traceback

    try:
        module = importlib.import_module(module_name)
        entry = vars(module).get("__program", vars(module)).get("main")
        if entry is None:
            sys.stderr.write("the program has no `fn main()`\n")
            return 2
        result = entry()
    except Exception as error:  # noqa: BLE001 - a panic is reported, not raised
        path = getattr(sys.modules.get(module_name), "__lotml__", None)
        lines = ["panic: " + type(error).__name__ + (f": {error}" if str(error) else "") + "\n"]
        for frame in traceback.extract_tb(error.__traceback__):
            if frame.filename.endswith(".lotml") and (path is None or frame.filename == path):
                lines.append(f'  File "{frame.filename}", line {frame.lineno}, in {frame.name}\n')
                if frame.line:
                    lines.append(f"    {frame.line}\n")
        sys.stderr.write("".join(lines))
        return 101
    if isinstance(result, Err):
        sys.stderr.write(f"error: {show(result.error)}\n")
        return 1
    return 0
