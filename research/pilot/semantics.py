"""Checks a parser cannot make: the mutability and truthiness rules of the X language."""

from functools import cache

from check import GRAMMAR, STRING_RE, VARIANTS, XIndenter
from lark import Lark, Token, Tree

MUTATING_METHODS = {
    "append",
    "extend",
    "insert",
    "pop",
    "remove",
    "clear",
    "sort",
    "reverse",
    "push",
    "pop_min",
    "update",
    "setdefault",
}
NUMERIC = {"int", "i8", "i16", "i32", "i64", "u8", "u16", "u32", "u64", "f32", "f64"}


@cache
def parser(variant: str) -> Lark:
    grammar = GRAMMAR.format(string_re=STRING_RE, **VARIANTS[variant])
    return Lark(grammar, parser="lalr", postlex=XIndenter(), maybe_placeholders=False)


class Scope:
    def __init__(self, parent: "Scope | None" = None) -> None:
        self.names: dict[str, tuple[bool, str | None]] = {}
        self.parent = parent

    def lookup(self, name: str) -> tuple[bool, str | None] | None:
        scope: Scope | None = self
        while scope is not None:
            if name in scope.names:
                return scope.names[name]
            scope = scope.parent
        return None


def category(node: Tree | Token | None) -> str | None:
    """A coarse static type for truthiness checks: bool, opt, list, dict, str, num, other."""
    if isinstance(node, Tree) and node.data == "type":
        if any(isinstance(c, Token) and c.type == "QMARK" for c in node.children):
            return "opt"
        base = node.children[0]
        if base.data == "named_type":
            name = str(base.children[0])
            if name == "bool":
                return "bool"
            return "num" if name in NUMERIC else "str" if name == "str" else "other"
        return {"list_type": "list", "dict_type": "dict", "tuple_type": "other"}.get(
            base.data
        )
    if isinstance(node, Tree) and node.data in ("list", "listcomp"):
        return "list"
    if isinstance(node, Tree) and node.data in ("dict", "dictcomp"):
        return "dict"
    if isinstance(node, Token) and node.type == "STRING":
        return "str"
    if isinstance(node, Token) and node.type == "NUMBER":
        return "num"
    return None


def root_name(node: Tree | Token) -> str | None:
    while isinstance(node, Tree) and node.data in ("getattr", "getitem", "call"):
        node = node.children[0]
    return str(node) if isinstance(node, Token) and node.type == "NAME" else None


class Checker:
    def __init__(self, variant: str, mutators: set[str]) -> None:
        self.variant = variant
        self.mutators = mutators
        self.found: list[tuple[str, str]] = []

    def mutable(self, name: str | None, scope: Scope) -> bool:
        binding = scope.lookup(name) if name else None
        return binding is None or binding[0]

    def function(self, fn: Tree) -> None:
        head, body = fn.children[0], fn.children[-1]
        scope = Scope()
        for params in head.find_data("params"):
            for param in params.children:
                is_var = any(
                    isinstance(c, Token) and c.type == "VAR" for c in param.children
                )
                name = next(
                    str(c)
                    for c in param.children
                    if isinstance(c, Token) and c.type == "NAME"
                )
                kind = next(
                    (
                        c
                        for c in param.children
                        if isinstance(c, Tree) and c.data == "type"
                    ),
                    None,
                )
                if is_var and name != "self":
                    self.found.append(("var parameter", name))
                scope.names[name] = (is_var, category(kind))
        self.block(body, Scope(scope))

    def block(self, suite: Tree, scope: Scope) -> None:
        for stmt in suite.children:
            self.statement(
                stmt.children[0] if stmt.data == "simple_stmt" else stmt, scope
            )

    def statement(self, stmt: Tree, scope: Scope) -> None:
        kids = stmt.children
        if stmt.data == "var_stmt":
            names = [c for c in kids if isinstance(c, Token) and c.type == "NAME"]
            kind = next(
                (c for c in kids if isinstance(c, Tree) and c.data == "type"), None
            )
            self.expression(kids[-1], scope)
            scope.names[str(names[0])] = (True, category(kind) or category(kids[-1]))
        elif stmt.data == "expr_stmt":
            self.assignment(kids, scope)
        elif stmt.data in ("if_stmt", "elif_clause", "while_stmt"):
            self.condition(kids[0], scope)
            for kid in kids[1:]:
                if kid.data == "suite":
                    self.block(kid, Scope(scope))
                else:
                    self.statement(kid, scope)
        elif stmt.data == "else_clause":
            self.block(kids[0], Scope(scope))
        elif stmt.data == "for_stmt":
            self.expression(kids[1], scope)
            inner = Scope(scope)
            for target in kids[0].children:
                inner.names[str(target)] = (False, None)
            self.block(kids[2], inner)
        elif stmt.data == "match_stmt":
            self.expression(kids[0], scope)
            for arm in kids[1:]:
                inner = Scope(scope)
                for token in arm.children[-2].scan_values(
                    lambda v: isinstance(v, Token)
                ):
                    if token.type == "NAME" and str(token)[:1].islower():
                        inner.names[str(token)] = (False, None)
                self.block(arm.children[-1], inner)
        else:
            for kid in kids:
                self.expression(kid, scope)

    def assignment(self, kids: list, scope: Scope) -> None:
        target = kids[0]
        if len(kids) == 1:
            self.expression(target, scope)
            return
        action = kids[1]
        self.expression(action.children[-1], scope)
        if action.data == "augassign":
            self.target(target, scope, declare=False, value=None)
        else:
            self.target(target, scope, declare=True, value=action.children[-1])

    def target(self, target, scope: Scope, declare: bool, value) -> None:
        if isinstance(target, Tree) and target.data == "testlist":
            for item in target.children:
                self.target(item, scope, declare, None)
        elif isinstance(target, Token) and target.type == "NAME":
            binding = scope.lookup(str(target))
            if binding is not None and not binding[0]:
                self.found.append(("reassign immutable", str(target)))
            elif binding is None and declare:
                scope.names[str(target)] = (False, category(value))
        elif isinstance(target, Tree):
            name = root_name(target)
            if not self.mutable(name, scope):
                self.found.append(("mutate immutable", str(name)))

    def expression(self, node, scope: Scope) -> None:
        if not isinstance(node, Tree):
            return
        for call in node.find_data("call"):
            callee = call.children[0]
            if isinstance(callee, Tree) and callee.data == "getattr":
                method = str(callee.children[1])
                name = root_name(callee.children[0])
                if method in self.mutators and not self.mutable(name, scope):
                    self.found.append(("mutate immutable", str(name)))

    def condition(self, node, scope: Scope) -> None:
        self.expression(node, scope)
        if isinstance(node, Tree) and node.data in ("not_op", "and_test", "or_test"):
            for kid in node.children:
                self.condition(kid, scope)
        elif isinstance(node, Token) and node.type == "NAME":
            binding = scope.lookup(str(node))
            kind = binding[1] if binding else None
            if kind in ("list", "dict", "str", "num", "other") or (
                kind == "opt" and self.variant == "b"
            ):
                self.found.append(("truthiness", str(node)))


def mutating_methods(tree: Tree) -> set[str]:
    """Methods declared with `var self` anywhere in the program."""
    names = set()
    for head in tree.find_data("fn_head"):
        for param in head.find_data("param"):
            tokens = [c for c in param.children if isinstance(c, Token)]
            if [t.type for t in tokens[:2]] == ["VAR", "NAME"] and str(
                tokens[1]
            ) == "self":
                names.add(str(head.children[0]))
    return names


def findings(variant: str, source: str) -> list[tuple[str, str]]:
    """Mutability and truthiness violations in a program that already parses."""
    if not source.endswith("\n"):
        source += "\n"
    tree = parser(variant).parse(source)
    checker = Checker(variant, MUTATING_METHODS | mutating_methods(tree))
    for fn in tree.find_data("fn_def"):
        checker.function(fn)
    for test in tree.find_data("test_def"):
        checker.block(test.children[-1], Scope())
    return checker.found
