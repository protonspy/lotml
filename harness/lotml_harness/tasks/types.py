"""lotml types as the task set needs them: read from Python annotations, written per variant."""

import ast
import re
from dataclasses import dataclass


class UnsupportedType(ValueError):
    """A type lotml cannot express, or text that is not a lotml type."""


PRIMITIVES = frozenset({"int", "f64", "str", "bool"})
PYTHON_PRIMITIVES = {"int": "int", "float": "f64", "str": "str", "bool": "bool"}


@dataclass(frozen=True)
class Prim:
    name: str

    def render(self, variant: str) -> str:
        return self.name


@dataclass(frozen=True)
class Unit:
    def render(self, variant: str) -> str:
        return "none" if variant == "a" else "None"


@dataclass(frozen=True)
class List:
    item: "Type"

    def render(self, variant: str) -> str:
        return f"[{self.item.render(variant)}]"


@dataclass(frozen=True)
class Set:
    item: "Type"

    def render(self, variant: str) -> str:
        return f"{{{self.item.render(variant)}}}"


@dataclass(frozen=True)
class Dict:
    key: "Type"
    value: "Type"

    def render(self, variant: str) -> str:
        return f"{{{self.key.render(variant)}: {self.value.render(variant)}}}"


@dataclass(frozen=True)
class Tuple:
    items: tuple["Type", ...]

    def render(self, variant: str) -> str:
        inner = ", ".join(item.render(variant) for item in self.items)
        return f"({inner},)" if len(self.items) == 1 else f"({inner})"


@dataclass(frozen=True)
class Optional:
    inner: "Type"

    def render(self, variant: str) -> str:
        return f"{self.inner.render(variant)}?"


type Type = Prim | Unit | List | Set | Dict | Tuple | Optional

GENERICS = {
    "List": "list",
    "list": "list",
    "Set": "set",
    "set": "set",
    "Dict": "dict",
    "dict": "dict",
    "Tuple": "tuple",
    "tuple": "tuple",
    "Optional": "optional",
}


def from_annotation(node: ast.expr) -> Type:
    """The lotml type of a Python annotation, or `UnsupportedType`."""
    if isinstance(node, ast.Constant) and node.value is None:
        return Unit()
    if isinstance(node, ast.Name):
        if node.id in PYTHON_PRIMITIVES:
            return Prim(PYTHON_PRIMITIVES[node.id])
        raise UnsupportedType(f"`{node.id}` has no lotml equivalent")
    if isinstance(node, ast.BinOp) and isinstance(node.op, ast.BitOr):
        sides = [node.left, node.right]
        nones = [s for s in sides if isinstance(s, ast.Constant) and s.value is None]
        if len(nones) == 1:
            other = next(s for s in sides if s is not nones[0])
            return Optional(from_annotation(other))
        raise UnsupportedType("a union other than `T | None`")
    if isinstance(node, ast.Subscript) and isinstance(node.value, ast.Name):
        kind = GENERICS.get(node.value.id)
        if kind is None:
            raise UnsupportedType(f"`{node.value.id}[...]` has no lotml equivalent")
        arguments = node.slice.elts if isinstance(node.slice, ast.Tuple) else [node.slice]
        if any(isinstance(a, ast.Constant) and a.value is ... for a in arguments):
            raise UnsupportedType("variable-length tuple")
        types = tuple(from_annotation(a) for a in arguments)
        expected = {"list": 1, "set": 1, "optional": 1, "dict": 2}.get(kind)
        if expected is not None and len(types) != expected:
            raise UnsupportedType(f"`{node.value.id}` takes {expected} arguments")
        match kind:
            case "list":
                return List(types[0])
            case "set":
                return Set(types[0])
            case "dict":
                return Dict(types[0], types[1])
            case "optional":
                return Optional(types[0])
            case _:
                return Tuple(types)
    raise UnsupportedType(f"`{ast.unparse(node)}` has no lotml equivalent")


WORD = re.compile(r"\s*(?:([A-Za-z_][A-Za-z0-9_]*)|(.))")


def parse(text: str) -> Type:
    """Read a type written in lotml syntax (variant B), as `render` writes it."""
    words = [m.group(1) or m.group(2) for m in WORD.finditer(text) if m.group(0).strip()]
    position = 0

    def peek() -> str | None:
        return words[position] if position < len(words) else None

    def take(expected: str | None = None) -> str:
        nonlocal position
        word = peek()
        if word is None or (expected is not None and word != expected):
            raise UnsupportedType(f"`{text}`: expected {expected or 'a type'}")
        position += 1
        return word

    def one() -> Type:
        word = take()
        if word in PRIMITIVES:
            result: Type = Prim(word)
        elif word == "None":
            result = Unit()
        elif word == "[":
            result = List(one())
            take("]")
        elif word == "{":
            first = one()
            if peek() == ":":
                take(":")
                result = Dict(first, one())
            else:
                result = Set(first)
            take("}")
        elif word == "(":
            items = [one()]
            while peek() == ",":
                take(",")
                if peek() == ")":
                    break
                items.append(one())
            take(")")
            result = Tuple(tuple(items))
        else:
            raise UnsupportedType(f"`{text}`: unknown type `{word}`")
        while peek() == "?":
            take("?")
            result = Optional(result)
        return result

    result = one()
    if peek() is not None:
        raise UnsupportedType(f"`{text}`: trailing `{peek()}`")
    return result
