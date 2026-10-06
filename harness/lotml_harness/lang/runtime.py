"""Runtime support for Python transpiled from lotml: results, values, the prelude, panics."""

import builtins
import copy
import dataclasses
import heapq
import math
import string
import sys
from typing import Any

I64_MIN, I64_MAX = -(2**63), 2**63 - 1


@dataclasses.dataclass(frozen=True)
class Ok:
    value: Any


@dataclasses.dataclass(frozen=True)
class Err:
    error: Any


class Fail(Exception):
    """Carries a lotml error from `fail` or `?` to the fallible function that returns it."""

    def __init__(self, error):
        super().__init__(error)
        self.error = error


class Panic(Exception):
    """A broken invariant: the program stops, as lotml's runtime stops it."""


class Overflow(Panic):
    """An `int` result outside i64: lotml traps in every build (R26)."""


class Todo(Panic):
    """`todo()` reached at run time."""


class OutputLimit(Panic):
    """A program printed more than a run allows."""


class NonExhaustiveMatch(Panic):
    """No arm matched: the case a lotml compiler rejects as a non-exhaustive `match`."""


class LotmlTypeError(TypeError):
    """A runtime check standing in for one of lotml's static type rules."""


def fail(error):
    raise Fail(error)


def unwrap(result):
    """`expr?`: the value of an `Ok`, or the error of an `Err` failing the caller."""
    if isinstance(result, Ok):
        return result.value
    if isinstance(result, Err):
        raise Fail(result.error)
    raise LotmlTypeError(f"`?` applied to a value that is not a result: {result!r}")


def coalesce(value, default):
    """`x ?? d`, with `d` a thunk so that `x ?? fail e` only fails on None."""
    return value if value is not None else default()


def no_match(subject):
    raise NonExhaustiveMatch(repr(subject))


def todo(*_args):
    raise Todo("not implemented")


def i64(value):
    """An integer result checked against i64; any other value passes through."""
    if type(value) is int and not I64_MIN <= value <= I64_MAX:
        raise Overflow(f"{value} overflows int")
    return value


def power(base, exponent):
    """`base ** exponent`, trapping an `int` result too wide for i64 before computing it."""
    if (
        type(base) is int
        and type(exponent) is int
        and exponent > 0
        and abs(base) > 1
        and (abs(base).bit_length() - 1) * exponent > 64
    ):
        raise Overflow(f"{base} ** {exponent} overflows int")
    return i64(base**exponent)


def lshift(value, amount):
    """`value << amount`, trapping a shift that leaves i64 before computing it."""
    if type(value) is int and type(amount) is int and value != 0 and amount >= 64:
        raise Overflow(f"{value} << {amount} overflows int")
    return i64(value << amount)


def wrap(value: int) -> int:
    return (value - I64_MIN) % 2**64 + I64_MIN


def wrapping_add(a: int, b: int) -> int:
    return wrap(a + b)


def wrapping_sub(a: int, b: int) -> int:
    return wrap(a - b)


def wrapping_mul(a: int, b: int) -> int:
    return wrap(a * b)


def condition(value):
    """Variant B: `if`, `while`, `and`, `or` and `not` only accept `bool`."""
    if not isinstance(value, bool):
        raise LotmlTypeError(f"condition is {type(value).__name__}, not bool")
    return value


def truth_a(value) -> bool:
    """Variant A: a condition is a `bool`, or an optional that holds a value."""
    if isinstance(value, bool):
        return value
    return value is not None


def or_a(left, right):
    """Variant A's `or`: boolean or on `bool`, the optional default otherwise."""
    if isinstance(left, bool):
        return left or truth_a(right())
    return left if left is not None else right()


IMMUTABLE = (int, float, str, bytes, bool, type(None), range)


def value(obj):
    """Value semantics: a mutable value is copied wherever it is bound or passed."""
    if isinstance(obj, (*IMMUTABLE, Unit, Box)) or callable(obj):
        return obj
    return copy.deepcopy(obj)


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


class Unit:
    """A variant without fields: one instance, equal only to itself, never copied."""

    __match_args__ = ()

    def __init__(self, name: str):
        self.name = name

    def __repr__(self):
        return self.name

    def __deepcopy__(self, memo):
        return self

    def __lt__(self, other):
        return isinstance(other, Unit) and self.name < other.name


class Variants:
    """The namespace value patterns refer to: `case Empty` matches `Variants.Empty`."""


def record(name: str, fields: tuple[str, ...], defaults: dict):
    """A record type; `defaults` maps a field to a thunk, so no default is shared.

    Types are erased, so `Stack[int]` is `Stack`.
    """
    spec = [
        (f, Any, dataclasses.field(default_factory=defaults[f])) if f in defaults else (f, Any)
        for f in fields
    ]
    erase_type_arguments = classmethod(lambda cls, _arguments: cls)
    cls = dataclasses.make_dataclass(
        name,
        spec,
        namespace={"__class_getitem__": erase_type_arguments},
        eq=True,
        order=True,
    )
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


class Heap:
    """The prelude's `Heap[T]`: a min-heap."""

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


def _to_number(text: str, kind):
    try:
        return kind(text.strip())
    except ValueError:
        return None


def _str_find(text: str, sub: str):
    at = text.find(sub)
    return None if at < 0 else at


def _split_once(text: str, sep: str):
    head, found, tail = text.partition(sep)
    return (head, tail) if found else None


def _list_pop(items: list, *index):
    if not items:
        return None
    return items.pop(*index)


def _list_find(items: list, predicate):
    return next((item for item in items if predicate(item)), None)


def _list_index(items: list, item):
    try:
        return items.index(item)
    except ValueError:
        return None


def _dict_pop(mapping: dict, key, *default):
    return mapping.pop(key, *default) if default else mapping.pop(key, None)


def _format_fields(text: str) -> list[str]:
    """Every replacement field of a format string, nested specs included."""
    fields = []
    for _literal, name, spec, _conversion in string.Formatter().parse(text):
        if name is not None:
            fields.append(name)
        if spec:
            fields += _format_fields(spec)
    return fields


def _safe_format(text: str, *args, format_map: bool = False, **kwargs):
    """`str.format`, refusing a field that walks into a dunder attribute: `{0.__class__}`."""
    if any("__" in field for field in _format_fields(text)):
        raise LotmlTypeError("a format field names a dunder attribute")
    return text.format_map(*args) if format_map else text.format(*args, **kwargs)


SPECIAL = {
    str: {
        "to_int": lambda s: _to_number(s, int),
        "to_float": lambda s: _to_number(s, float),
        "find": _str_find,
        "split_once": _split_once,
        "format": _safe_format,
        "format_map": lambda s, mapping: _safe_format(s, mapping, format_map=True),
    },
    list: {
        "pop": _list_pop,
        "last": lambda xs: xs[-1] if xs else None,
        "find": _list_find,
        "contains": lambda xs, v: v in xs,
        "index": _list_index,
    },
    dict: {
        "pop": _dict_pop,
        "contains": lambda d, k: k in d,
        "keys": lambda d: list(d.keys()),
        "values": lambda d: list(d.values()),
        "items": lambda d: list(d.items()),
    },
}

SPECIAL_NAMES = frozenset(name for methods in SPECIAL.values() for name in methods)


def method(obj, name: str, *args, **kwargs):
    """Call a method whose lotml behavior differs from Python's built-in one."""
    special = SPECIAL.get(type(obj), {}).get(name)
    if special is not None:
        return special(obj, *args, **kwargs)
    return getattr(obj, name)(*args, **kwargs)


OUTPUT_LIMIT = 1_000_000
"""Characters one program may print."""


def capped_print(limit: int = OUTPUT_LIMIT):
    """A `print` that stops the program once it has written `limit` characters."""
    written = 0

    def print_(*values, sep=" ", end="\n"):
        nonlocal written
        text = sep.join(str(v) for v in values) + end
        written += len(text)
        if written > limit:
            raise OutputLimit(f"the program printed more than {limit} characters")
        sys.stdout.write(text)

    print_.__name__ = "print"
    return print_


def _listed(function):
    def listed(*args, **kwargs):
        return list(function(*args, **kwargs))

    listed.__name__ = function.__name__
    return listed


PRELUDE = {
    **{
        name: getattr(builtins, name)
        for name in (
            "print",
            "len",
            "range",
            "enumerate",
            "sorted",
            "sum",
            "min",
            "max",
            "abs",
            "any",
            "all",
            "round",
            "int",
            "float",
            "str",
            "bool",
            "ord",
            "chr",
            "set",
            "list",
            "dict",
            "tuple",
            "divmod",
            "pow",
            "hash",
        )
    },
    "reversed": _listed(reversed),
    "zip": _listed(zip),
    "map": _listed(map),
    "filter": _listed(filter),
    "Heap": Heap,
    "todo": todo,
    "Ok": Ok,
    "Err": Err,
    "wrapping_add": wrapping_add,
    "wrapping_sub": wrapping_sub,
    "wrapping_mul": wrapping_mul,
    "isqrt": math.isqrt,
    "gcd": math.gcd,
}
"""The names every program sees without an import (R35)."""
