"""Values of lotml types: checked against a type, written as lotml literals, kept as JSON."""

import json
import math
from typing import Any

from lotml_harness.tasks.types import Dict, List, Optional, Prim, Set, Tuple, Type, Unit

I64_MIN, I64_MAX = -(2**63), 2**63 - 1


class Mismatch(ValueError):
    """A value that does not belong to the type it was given."""


def conform(value: Any, type_: Type) -> Any:
    """`value` as the Python value lotml's runtime uses for `type_`, or `Mismatch`.

    A sequence becomes a list, a tuple or a set as the type says, and an integer an `f64`
    where a float is expected — the conversions a lotml literal of that type makes.
    """
    match type_:
        case Prim("int"):
            if not isinstance(value, int) or isinstance(value, bool):
                raise Mismatch(f"{value!r} is not an int")
            if not I64_MIN <= value <= I64_MAX:
                raise Mismatch(f"{value} overflows i64")
            return value
        case Prim("f64"):
            if isinstance(value, bool) or not isinstance(value, int | float):
                raise Mismatch(f"{value!r} is not an f64")
            if not math.isfinite(value):
                raise Mismatch(f"{value!r} has no lotml literal")
            return float(value)
        case Prim("str"):
            if not isinstance(value, str):
                raise Mismatch(f"{value!r} is not a str")
            return value
        case Prim("bool"):
            if not isinstance(value, bool):
                raise Mismatch(f"{value!r} is not a bool")
            return value
        case Unit():
            if value is not None:
                raise Mismatch(f"{value!r} is not None")
            return None
        case Optional(inner):
            return None if value is None else conform(value, inner)
        case List(item):
            if not isinstance(value, list | tuple):
                raise Mismatch(f"{value!r} is not a list")
            return [conform(v, item) for v in value]
        case Set(item):
            if not isinstance(value, set | frozenset | list | tuple):
                raise Mismatch(f"{value!r} is not a set")
            items = [conform(v, item) for v in value]
            result = set(items)
            if len(result) != len(items):
                raise Mismatch(f"{value!r} repeats an element of a set")
            return result
        case Dict(key, item):
            if not isinstance(value, dict):
                raise Mismatch(f"{value!r} is not a dict")
            return {conform(k, key): conform(v, item) for k, v in value.items()}
        case Tuple(items):
            if not isinstance(value, list | tuple) or len(value) != len(items):
                raise Mismatch(f"{value!r} is not a {len(items)}-tuple")
            return tuple(conform(v, t) for v, t in zip(value, items, strict=True))
    raise Mismatch(f"unknown type {type_!r}")


def render(value: Any, type_: Type, variant: str) -> str:
    """`value`, already conformed to `type_`, as a lotml literal."""
    match type_:
        case Prim("int") | Prim("bool"):
            return repr(value)
        case Prim("f64"):
            return repr(float(value))
        case Prim("str"):
            return json.dumps(value, ensure_ascii=False)
        case Unit():
            return Unit().render(variant)
        case Optional(inner):
            return Unit().render(variant) if value is None else render(value, inner, variant)
        case List(item):
            return "[" + ", ".join(render(v, item, variant) for v in value) + "]"
        case Set(item):
            if not value:
                return "set()"
            return "{" + ", ".join(render(v, item, variant) for v in sorted(value)) + "}"
        case Dict(key, item):
            pairs = (
                f"{render(k, key, variant)}: {render(v, item, variant)}" for k, v in value.items()
            )
            return "{" + ", ".join(pairs) + "}"
        case Tuple(items):
            parts = [render(v, t, variant) for v, t in zip(value, items, strict=True)]
            return f"({parts[0]},)" if len(parts) == 1 else "(" + ", ".join(parts) + ")"
    raise Mismatch(f"unknown type {type_!r}")


def to_json(value: Any, type_: Type) -> Any:
    """`value` as JSON data; sets and tuples become lists and dicts lists of pairs."""
    match type_:
        case Optional(inner):
            return None if value is None else to_json(value, inner)
        case List(item):
            return [to_json(v, item) for v in value]
        case Set(item):
            return [to_json(v, item) for v in sorted(value)]
        case Dict(key, item):
            return [[to_json(k, key), to_json(v, item)] for k, v in value.items()]
        case Tuple(items):
            return [to_json(v, t) for v, t in zip(value, items, strict=True)]
    return value


def from_json(data: Any, type_: Type) -> Any:
    """The inverse of `to_json`, conformed to `type_`."""
    match type_:
        case Optional(inner):
            return None if data is None else from_json(data, inner)
        case List(item):
            return [from_json(v, item) for v in data]
        case Set(item):
            return {from_json(v, item) for v in data}
        case Dict(key, item):
            return {from_json(k, key): from_json(v, item) for k, v in data}
        case Tuple(items):
            return tuple(from_json(v, t) for v, t in zip(data, items, strict=True))
    return conform(data, type_)
