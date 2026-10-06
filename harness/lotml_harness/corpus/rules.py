"""The mechanical part of translating Python into lotml.

Where lotml's syntax is Python's (variant B), an expression is written back as Python writes it.
What differs by rule is translated: `def` becomes `fn` with lotml types, a local assigned again or
changed in place is declared `var` at its first assignment, a parameter the body changes is
taken `var`, the `typing` imports go, and a list, string, dict, set or number tested for its
truth is compared explicitly. What is not mechanical — exceptions, classes, a `find` that
returns `-1`, an `int(text)` — is left to the model, with the reason recorded.
"""

import ast
from dataclasses import dataclass, field

from lotml_harness.tasks import Task, types

MUTATING = {
    "append",
    "extend",
    "insert",
    "pop",
    "remove",
    "sort",
    "reverse",
    "clear",
    "add",
    "discard",
    "update",
    "setdefault",
    "popitem",
}
"""Methods that change the list, dict or set they are called on."""

MATH = {"sqrt", "floor", "ceil", "pow", "log", "exp", "sin", "cos", "pi", "inf", "gcd", "isqrt"}
"""What lotml's own `math` module has."""

UNSUPPORTED = {
    ast.ClassDef: "a class",
    ast.Try: "an exception handler",
    ast.Raise: "a `raise`",
    ast.With: "a `with` block",
    ast.Global: "a `global`",
    ast.Nonlocal: "a `nonlocal`",
    ast.Yield: "a generator",
    ast.YieldFrom: "a generator",
    ast.Await: "a coroutine",
    ast.AsyncFunctionDef: "a coroutine",
    ast.Delete: "a `del`",
    ast.NamedExpr: "a `:=`",
    ast.Starred: "a starred expression",
}

COLLECTIONS = ("list", "str", "dict", "set")


@dataclass
class Translation:
    """A program in lotml, or what kept the rules from writing one."""

    code: str | None
    reasons: list[str] = field(default_factory=list)


class Unsupported(Exception):
    """A construct the rules do not translate."""


def lotml_type(node: ast.expr | None) -> str:
    """A Python annotation as a lotml type."""
    if node is None:
        raise Unsupported("a parameter or result with no type")
    if isinstance(node, ast.Constant) and node.value is None:
        return "None"
    if isinstance(node, ast.Name):
        simple = {"int": "int", "float": "f64", "str": "str", "bool": "bool"}
        if node.id in simple:
            return simple[node.id]
        raise Unsupported(f"the type `{node.id}`")
    if isinstance(node, ast.BinOp) and isinstance(node.op, ast.BitOr):
        if isinstance(node.right, ast.Constant) and node.right.value is None:
            return f"{lotml_type(node.left)}?"
        raise Unsupported(f"the union `{ast.unparse(node)}`")
    if isinstance(node, ast.Subscript) and isinstance(node.value, ast.Name):
        head = node.value.id
        args = node.slice.elts if isinstance(node.slice, ast.Tuple) else [node.slice]
        if head == "Optional":
            return f"{lotml_type(args[0])}?"
        if head in ("List", "list"):
            return f"[{lotml_type(args[0])}]"
        if head in ("Set", "set"):
            return f"{{{lotml_type(args[0])}}}"
        if head in ("Dict", "dict") and len(args) == 2:
            return f"{{{lotml_type(args[0])}: {lotml_type(args[1])}}}"
        if head in ("Tuple", "tuple"):
            return "(" + ", ".join(lotml_type(a) for a in args) + ")"
    raise Unsupported(f"the type `{ast.unparse(node)}`")


def kind(t: types.Type) -> str | None:
    """What a value of task type `t` is tested for its truth as."""
    match t:
        case types.List() | types.Dict() | types.Set():
            return type(t).__name__.lower()
        case types.Prim("str"):
            return "str"
        case types.Prim("int") | types.Prim("f64"):
            return "number"
        case types.Prim("bool"):
            return "bool"
        case types.Optional():
            return "optional"
    return None


def literal_kind(node: ast.expr) -> str | None:
    """What a value is, when its expression says so on its face."""
    if isinstance(node, (ast.List, ast.ListComp)):
        return "list"
    if isinstance(node, (ast.Dict, ast.DictComp)):
        return "dict"
    if isinstance(node, (ast.Set, ast.SetComp)):
        return "set"
    if isinstance(node, ast.JoinedStr):
        return "str"
    if isinstance(node, ast.Constant):
        if isinstance(node.value, bool):
            return "bool"
        if isinstance(node.value, (int, float)):
            return "number"
        if isinstance(node.value, str):
            return "str"
    if isinstance(node, ast.Call) and isinstance(node.func, ast.Name):
        return {
            "len": "number",
            "list": "list",
            "set": "set",
            "dict": "dict",
            "sorted": "list",
        }.get(node.func.id)
    if isinstance(node, ast.Compare) or (
        isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.Not)
    ):
        return "bool"
    return None


class Truth(ast.NodeTransformer):
    """`if xs:` as `if len(xs) > 0:`: lotml tests only a `bool`."""

    def __init__(self, kinds: dict[str, str]):
        self.kinds = kinds

    def explicit(self, node: ast.expr, negated: bool = False) -> ast.expr:
        if isinstance(node, ast.UnaryOp) and isinstance(node.op, ast.Not):
            return self.explicit(node.operand, not negated)
        if isinstance(node, ast.BoolOp):
            values = [self.explicit(v) for v in node.values]
            combined = ast.BoolOp(op=node.op, values=values)
            return ast.UnaryOp(op=ast.Not(), operand=combined) if negated else combined
        node = self.visit(node)
        what = self.kinds.get(node.id) if isinstance(node, ast.Name) else None
        if what in COLLECTIONS:
            size = ast.Call(func=ast.Name("len"), args=[node], keywords=[])
            op = ast.Eq() if negated else ast.Gt()
            return ast.Compare(left=size, ops=[op], comparators=[ast.Constant(0)])
        if what == "number":
            op = ast.Eq() if negated else ast.NotEq()
            return ast.Compare(left=node, ops=[op], comparators=[ast.Constant(0)])
        if what == "optional":
            op = ast.Is() if negated else ast.IsNot()
            return ast.Compare(left=node, ops=[op], comparators=[ast.Constant(None)])
        return ast.UnaryOp(op=ast.Not(), operand=node) if negated else node

    def visit_If(self, node: ast.If) -> ast.AST:
        node.test = self.explicit(node.test)
        node.body = [self.visit(s) for s in node.body]
        node.orelse = [self.visit(s) for s in node.orelse]
        return node

    visit_While = visit_If

    def visit_IfExp(self, node: ast.IfExp) -> ast.AST:
        node.test = self.explicit(node.test)
        node.body = self.visit(node.body)
        node.orelse = self.visit(node.orelse)
        return node

    def visit_UnaryOp(self, node: ast.UnaryOp) -> ast.AST:
        if isinstance(node.op, ast.Not):
            return self.explicit(node)
        return self.generic_visit(node)

    def visit_comprehension(self, node: ast.comprehension) -> ast.AST:
        node.iter = self.visit(node.iter)
        node.ifs = [self.explicit(i) for i in node.ifs]
        return node


def changed(function: ast.FunctionDef) -> tuple[dict[str, int], set[str]]:
    """How many times each name is assigned, and the names changed in place: a local assigned
    more than once needs `var`, and so does a parameter assigned at all."""
    counts: dict[str, int] = {}
    in_place: set[str] = set()

    def bind(target: ast.expr, times: int = 1) -> None:
        if isinstance(target, ast.Name):
            counts[target.id] = counts.get(target.id, 0) + times
        elif isinstance(target, (ast.Tuple, ast.List)):
            for item in target.elts:
                bind(item, times)
        elif isinstance(target, (ast.Subscript, ast.Attribute)):
            root = target
            while isinstance(root, (ast.Subscript, ast.Attribute)):
                root = root.value
            if isinstance(root, ast.Name):
                in_place.add(root.id)

    for node in ast.walk(function):
        if isinstance(node, ast.Assign):
            for target in node.targets:
                bind(target)
        elif isinstance(node, (ast.AugAssign, ast.AnnAssign)):
            bind(node.target, 2 if isinstance(node, ast.AugAssign) else 1)
        elif isinstance(node, ast.For):
            # A loop variable is bound by the loop each time, which lotml allows.
            continue
        elif (
            isinstance(node, ast.Call)
            and isinstance(node.func, ast.Attribute)
            and node.func.attr in MUTATING
        ):
            root = node.func.value
            while isinstance(root, (ast.Subscript, ast.Attribute)):
                root = root.value
            if isinstance(root, ast.Name):
                in_place.add(root.id)
    return counts, in_place


class Writer:
    """One function's statements as lotml lines."""

    def __init__(self, function: ast.FunctionDef, params: list[str], kinds: dict[str, str]):
        self.assigned, self.in_place = changed(function)
        self.again = {n for n, c in self.assigned.items() if c > 1}
        self.declared: set[str] = set(params)
        self.kinds = kinds
        self.lines: list[str] = []

    def expr(self, node: ast.expr) -> str:
        for inner in ast.walk(node):
            for kind_, why in UNSUPPORTED.items():
                if isinstance(inner, kind_):
                    raise Unsupported(why)
        return ast.unparse(node)

    def write(self, depth: int, text: str) -> None:
        self.lines.append("    " * depth + text)

    def block(self, body: list[ast.stmt], depth: int) -> None:
        for stmt in body:
            self.stmt(stmt, depth)

    def bound(self, target: ast.expr) -> str:
        """A binding's target, `var` when its name is new here and changes later."""
        if isinstance(target, ast.Name) and target.id not in self.declared:
            self.declared.add(target.id)
            if target.id in self.again or target.id in self.in_place:
                return f"var {target.id}"
        elif isinstance(target, ast.Tuple):
            names = [e.id for e in target.elts if isinstance(e, ast.Name)]
            if any(n in self.again or n in self.in_place for n in names) and not all(
                n in self.declared for n in names
            ):
                raise Unsupported("a tuple assigned to new names that change later")
            self.declared.update(names)
        return self.expr(target)

    def stmt(self, stmt: ast.stmt, depth: int) -> None:
        for kind_, why in UNSUPPORTED.items():
            if isinstance(stmt, kind_):
                raise Unsupported(why)
        match stmt:
            case ast.Expr(value=ast.Constant(value=str() as doc)) if not self.lines:
                text = doc.strip()
                if '"""' in text or "\\" in text or text.endswith('"'):
                    self.write(depth, repr(text))
                else:
                    self.write(depth, f'"""{text}"""')
            case ast.Expr(value=value):
                self.write(depth, self.expr(value))
            case ast.Assign(targets=[target], value=value):
                value_text = self.expr(value)
                self.write(depth, f"{self.bound(target)} = {value_text}")
            case ast.Assign():
                raise Unsupported("a chained assignment")
            case ast.AnnAssign(target=ast.Name(id=name), annotation=annotation, value=value):
                if value is None:
                    raise Unsupported("a declaration with no value")
                keyword = "var " if name in self.again or name in self.in_place else ""
                self.declared.add(name)
                self.write(depth, f"{keyword}{name}: {lotml_type(annotation)} = {self.expr(value)}")
            case ast.AugAssign():
                self.write(depth, self.expr(stmt))
            case ast.Return(value=None):
                self.write(depth, "return")
            case ast.Return(value=value):
                self.write(depth, f"return {self.expr(value)}")
            case ast.If():
                self.write(depth, f"if {self.expr(stmt.test)}:")
                self.block(stmt.body, depth + 1)
                orelse = stmt.orelse
                while len(orelse) == 1 and isinstance(orelse[0], ast.If):
                    self.write(depth, f"elif {self.expr(orelse[0].test)}:")
                    self.block(orelse[0].body, depth + 1)
                    orelse = orelse[0].orelse
                if orelse:
                    self.write(depth, "else:")
                    self.block(orelse, depth + 1)
            case ast.For(orelse=[_, *_]) | ast.While(orelse=[_, *_]):
                raise Unsupported("a loop with an `else`")
            case ast.For(target=target, iter=iterable, body=body):
                if isinstance(target, ast.Name):
                    self.declared.add(target.id)
                self.write(depth, f"for {self.expr(target)} in {self.expr(iterable)}:")
                self.block(body, depth + 1)
            case ast.While(test=test, body=body):
                self.write(depth, f"while {self.expr(test)}:")
                self.block(body, depth + 1)
            case ast.Assert(test=test, msg=None):
                self.write(depth, f"assert {self.expr(test)}")
            case ast.Pass():
                self.write(depth, "pass")
            case ast.Break():
                self.write(depth, "break")
            case ast.Continue():
                self.write(depth, "continue")
            case ast.FunctionDef():
                raise Unsupported("a nested function")
            case ast.Import() | ast.ImportFrom():
                raise Unsupported("an import inside a function")
            case _:
                raise Unsupported(f"the statement `{type(stmt).__name__}`")


def function(
    node: ast.FunctionDef, signature: tuple[list[tuple[str, str]], str] | None, task: Task | None
) -> list[str]:
    """A function as lotml lines: its signature from the task when it is the task's, else from
    its annotations."""
    if node.decorator_list:
        raise Unsupported("a decorator")
    arguments = node.args
    if arguments.vararg or arguments.kwarg or arguments.kwonlyargs or arguments.posonlyargs:
        raise Unsupported("`*args`, `**kwargs` or keyword-only parameters")
    if signature is None:
        params = [(a.arg, lotml_type(a.annotation)) for a in arguments.args]
        returns = lotml_type(node.returns) if node.returns is not None else "None"
    else:
        params, returns = signature
    if len(params) != len(arguments.args):
        raise Unsupported("a signature that differs from the task's")
    for unsupported in ast.walk(ast.Module(body=list(arguments.defaults), type_ignores=[])):
        for kind_, why in UNSUPPORTED.items():
            if isinstance(unsupported, kind_):
                raise Unsupported(why)
    defaults = [None] * (len(params) - len(arguments.defaults)) + [
        ast.unparse(d) for d in arguments.defaults
    ]
    kinds: dict[str, str] = {}
    if task is not None:
        for name, t in task.params:
            if (k := kind(t)) is not None:
                kinds[name] = k
    else:
        for arg in arguments.args:
            k = lotml_kind(arg.annotation)
            if k is not None:
                kinds[arg.arg] = k
    # A local's kind from its first assignment, when that says it on its face.
    for inner in ast.walk(node):
        if (
            isinstance(inner, ast.Assign)
            and len(inner.targets) == 1
            and isinstance(inner.targets[0], ast.Name)
            and (k := literal_kind(inner.value)) is not None
        ):
            kinds.setdefault(inner.targets[0].id, k)
    writer = Writer(node, [n for n, _ in params], kinds)
    changed_params = {n for n, _ in params if n in writer.assigned or n in writer.in_place}
    rendered = ", ".join(
        f"{'var ' if n in changed_params else ''}{n}: {t}{'' if d is None else f' = {d}'}"
        for (n, t), d in zip(params, defaults, strict=True)
    )
    arrow = "" if returns == "None" else f" -> {returns}"
    body = Truth(writer.kinds).visit(ast.Module(body=node.body, type_ignores=[])).body
    writer.block(body, 1)
    return [f"fn {node.name}({rendered}){arrow}:", *writer.lines]


def lotml_kind(annotation: ast.expr | None) -> str | None:
    if annotation is None:
        return None
    try:
        text = lotml_type(annotation)
    except Unsupported:
        return None
    if text.endswith("?"):
        return "optional"
    if text.startswith("["):
        return "list"
    if text.startswith("{"):
        return "dict" if ":" in text else "set"
    return {"int": "number", "f64": "number", "str": "str", "bool": "bool"}.get(text)


def translate(task: Task) -> Translation:
    """The task's canonical Python as lotml, or the reasons the rules could not write it."""
    try:
        tree = ast.parse(task.canonical)
    except SyntaxError as error:
        return Translation(None, [f"the Python does not parse: {error.msg}"])
    imports: list[str] = []
    items: list[list[str]] = []
    reasons: list[str] = []
    signature = (
        [(n, t.render("b")) for n, t in task.params],
        "None" if task.returns == types.Unit() else task.returns.render("b"),
    )
    for node in tree.body:
        try:
            match node:
                case ast.ImportFrom(module="typing") | ast.Import(names=[ast.alias(name="typing")]):
                    continue
                case ast.Import(names=[ast.alias(name="math", asname=None)]):
                    imports.append("import math")
                case ast.ImportFrom(module="math", names=names) if all(
                    n.name in MATH and n.asname is None for n in names
                ):
                    imports.append(f"from math import {', '.join(n.name for n in names)}")
                case ast.Import() | ast.ImportFrom():
                    raise Unsupported(f"the import `{ast.unparse(node)}`")
                case ast.FunctionDef(name=name) if name == task.name:
                    items.append(function(node, signature, task))
                case ast.FunctionDef():
                    items.append(function(node, None, None))
                case ast.Expr(value=ast.Constant(value=str())):
                    continue
                case _:
                    raise Unsupported("a statement at module level")
        except Unsupported as why:
            reasons.append(str(why))
    if reasons:
        return Translation(None, sorted(set(reasons)))
    parts = ["\n".join(imports)] if imports else []
    parts += ["\n".join(lines) for lines in items]
    return Translation("\n\n".join(parts) + "\n", [])
