from dataclasses import dataclass


@dataclass
class Null:
    pass


@dataclass
class Bool:
    value: bool


@dataclass
class Num:
    value: float


@dataclass
class Str:
    value: str


@dataclass
class Arr:
    items: list["Json"]


@dataclass
class Obj:
    fields: dict[str, "Json"]


type Json = Null | Bool | Num | Str | Arr | Obj


def render(j: Json) -> str:
    match j:
        case Null():
            return "null"
        case Bool(value):
            return "true" if value else "false"
        case Num(value):
            return str(value)
        case Str(value):
            return f'"{value}"'
        case Arr(items):
            return "[" + ", ".join(render(i) for i in items) + "]"
        case Obj(fields):
            pairs = [f'"{k}": {render(v)}' for k, v in fields.items()]
            return "{" + ", ".join(pairs) + "}"


def depth(j: Json) -> int:
    match j:
        case Arr(items):
            return 1 + max((depth(i) for i in items), default=0)
        case Obj(fields):
            return 1 + max((depth(v) for v in fields.values()), default=0)
        case _:
            return 0


def test_render_and_depth() -> None:
    doc = Obj({"a": Arr([Num(1.0), Null()]), "b": Bool(True)})
    assert render(doc) == '{"a": [1.0, null], "b": true}'
    assert depth(doc) == 2
