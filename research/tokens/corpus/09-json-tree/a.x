type Json = Null | Bool(value: bool) | Num(value: f64) | Str(value: str) | Arr(items: [Json]) | Obj(fields: {str: Json})

fn render(j: Json) -> str:
    match j:
        Null:
            return "null"
        Bool(value):
            return "true" if value else "false"
        Num(value):
            return str(value)
        Str(value):
            return f'"{value}"'
        Arr(items):
            return "[" + ", ".join(render(i) for i in items) + "]"
        Obj(fields):
            pairs = [f'"{k}": {render(v)}' for k, v in fields.items()]
            return "{" + ", ".join(pairs) + "}"

fn depth(j: Json) -> int:
    match j:
        Arr(items):
            return 1 + max((depth(i) for i in items), default=0)
        Obj(fields):
            return 1 + max((depth(v) for v in fields.values()), default=0)
        _:
            return 0

test "render and depth":
    doc = Obj({"a": Arr([Num(1.0), Null]), "b": Bool(True)})
    assert render(doc) == '{"a": [1.0, null], "b": true}'
    assert depth(doc) == 2
