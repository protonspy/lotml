"""Runtime support for Python code transpiled from lotml's pilot subset."""

import copy
import dataclasses
import heapq
from typing import Any


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


class NonExhaustiveMatch(Exception):
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


def value(obj):
    """Value semantics: a mutable value is copied wherever it is bound or passed."""
    if isinstance(obj, (int, float, str, bytes, bool, type(None), Unit)):
        return obj
    return copy.deepcopy(obj)


class Unit:
    """A variant without fields: one instance, equal only to itself, never copied."""

    __match_args__ = ()

    def __init__(self, name: str):
        self.name = name

    def __repr__(self):
        return self.name

    def __deepcopy__(self, memo):
        return self


class Variants:
    """The namespace value patterns refer to: `case Empty` matches `Variants.Empty`."""


def record(name: str, fields: tuple[str, ...], defaults: dict):
    """A record type; `defaults` maps a field to a thunk, so no default is shared.

    Types are erased, so `Stack[int]` is `Stack`.
    """
    spec = [
        (f, Any, dataclasses.field(default_factory=defaults[f]))
        if f in defaults
        else (f, Any)
        for f in fields
    ]
    erase_type_arguments = classmethod(lambda cls, _arguments: cls)
    return dataclasses.make_dataclass(
        name,
        spec,
        namespace={"__class_getitem__": erase_type_arguments},
        eq=True,
        unsafe_hash=True,
    )


def attach(target, name: str, function, is_method: bool):
    setattr(target, name, function if is_method else staticmethod(function))


class Heap:
    """The spec's `Heap[T]`: `Heap(items)`, `push`, `pop_min` and `len`."""

    def __init__(self, items=()):
        self.items = list(items)
        heapq.heapify(self.items)

    def push(self, item):
        heapq.heappush(self.items, item)

    def pop_min(self):
        return heapq.heappop(self.items)

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


def _dict_pop(mapping: dict, key, *default):
    return mapping.pop(key, *default) if default else mapping.pop(key, None)


SPECIAL = {
    str: {
        "to_int": lambda s: _to_number(s, int),
        "to_float": lambda s: _to_number(s, float),
        "find": _str_find,
        "split_once": _split_once,
    },
    list: {
        "pop": _list_pop,
        "last": lambda xs: xs[-1] if xs else None,
        "find": _list_find,
        "contains": lambda xs, v: v in xs,
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
    """Call a method whose spec behavior differs from Python's built-in one."""
    special = SPECIAL.get(type(obj), {}).get(name)
    if special is not None:
        return special(obj, *args, **kwargs)
    return getattr(obj, name)(*args, **kwargs)
