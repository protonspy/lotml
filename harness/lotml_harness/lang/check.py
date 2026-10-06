"""What lotml's compiler rejects that the parser accepts, and Python constructs that leak in.

`violations` finds the mutability errors a model's Python habits produce — reassigning or
mutating an immutable, passing an immutable as `&x` — which a type checker would reject
and the executor, with types erased, would run. `leaks` finds Python constructs the
variant does not have, by pattern, as the pilot did.
"""

import re

from lark import Token, Tree

from lotml_harness.lang.grammar import STRING_RE, parser

MUTATING_METHODS = frozenset(
    {
        "append",
        "extend",
        "insert",
        "pop",
        "remove",
        "discard",
        "add",
        "clear",
        "sort",
        "reverse",
        "push",
        "pop_min",
        "update",
        "setdefault",
    }
)


class Scope:
    def __init__(self, parent: "Scope | None" = None) -> None:
        self.names: dict[str, bool] = {}
        self.parent = parent

    def lookup(self, name: str) -> bool | None:
        """Whether `name` is mutable, or None when it is not declared here."""
        scope: Scope | None = self
        while scope is not None:
            if name in scope.names:
                return scope.names[name]
            scope = scope.parent
        return None


def bare(node):
    """Skip the wrappers placeholders leave: `test` and `power` with nothing optional."""
    while (
        isinstance(node, Tree)
        and node.data in ("test", "power")
        and all(child is None for child in node.children[1:])
    ):
        node = node.children[0]
    return node


def root_name(node) -> str | None:
    node = bare(node)
    while isinstance(node, Tree) and node.data in ("getattr", "getitem", "call", "try_op"):
        node = bare(node.children[0])
    return node.value if isinstance(node, Token) and node.type == "NAME" else None


def target_names(node) -> list[str]:
    if isinstance(node, Token):
        return [node.value]
    return [name for child in node.children for name in target_names(child)]


class Checker:
    def __init__(self, mutators: set[str]) -> None:
        self.mutators = mutators
        self.found: list[str] = []

    def immutable(self, name: str | None, scope: Scope) -> bool:
        return name is not None and scope.lookup(name) is False

    def function(self, fn: Tree) -> None:
        head, body = fn.children
        scope = Scope()
        params = head.children[2]
        for param in params.children if params is not None else []:
            kind = param.children[0]
            scope.names[param.children[1].value] = isinstance(kind, Token) and kind.type in (
                "INOUT",
                "VAR",
            )
        self.block(body, Scope(scope))

    def block(self, suite: Tree, scope: Scope) -> None:
        for statement in suite.children:
            self.statement(
                statement.children[0] if statement.data == "simple_stmt" else statement, scope
            )

    def statement(self, node: Tree, scope: Scope) -> None:
        kids = node.children
        if node.data == "var_stmt":
            self.expression(kids[-1], scope)
            scope.names[kids[1].value] = True
        elif node.data == "expr_stmt":
            self.assignment(kids, scope)
        elif node.data in ("if_stmt", "elif_clause", "while_stmt"):
            self.expression(kids[0], scope)
            for kid in kids[1:]:
                if kid is None:
                    continue
                if kid.data == "suite":
                    self.block(kid, Scope(scope))
                else:
                    self.statement(kid, scope)
        elif node.data == "else_clause":
            self.block(kids[0], Scope(scope))
        elif node.data == "for_stmt":
            self.expression(kids[1], scope)
            inner = Scope(scope)
            for name in target_names(kids[0]):
                inner.names[name] = False
            self.block(kids[2], inner)
        elif node.data == "match_stmt":
            self.expression(kids[0], scope)
            for arm in kids[1:]:
                inner = Scope(scope)
                for token in arm.children[-2].scan_values(lambda v: isinstance(v, Token)):
                    if token.type == "NAME" and token.value[:1].islower():
                        inner.names[token.value] = False
                self.block(arm.children[-1], inner)
        else:
            for kid in kids:
                self.expression(kid, scope)

    def assignment(self, kids: list, scope: Scope) -> None:
        target, action = kids[0], kids[1] if len(kids) > 1 else None
        if action is None:
            self.expression(target, scope)
            return
        self.expression(action.children[-1], scope)
        self.target(target, scope, declare=action.data != "augassign")

    def target(self, target, scope: Scope, declare: bool) -> None:
        target = bare(target)
        if isinstance(target, Tree) and target.data == "testlist":
            for item in target.children:
                self.target(item, scope, declare)
        elif isinstance(target, Token) and target.type == "NAME":
            mutable = scope.lookup(target.value)
            if mutable is False:
                self.found.append(f"reassign immutable `{target.value}`")
            elif mutable is None and declare:
                scope.names[target.value] = False
        elif isinstance(target, Tree):
            name = root_name(target)
            if self.immutable(name, scope):
                self.found.append(f"mutate immutable `{name}`")

    def expression(self, node, scope: Scope) -> None:
        if not isinstance(node, Tree):
            return
        for call in node.find_data("call"):
            callee = call.children[0]
            if isinstance(callee, Tree) and callee.data == "getattr":
                name = root_name(callee.children[0])
                if callee.children[1].value in self.mutators and self.immutable(name, scope):
                    self.found.append(f"mutate immutable `{name}`")
        for argument in node.find_data("inout_arg"):
            name = root_name(argument.children[0])
            if self.immutable(name, scope):
                self.found.append(f"`&{name}` of an immutable")


def mutating_methods(tree: Tree) -> set[str]:
    """Methods declared with `inout self` (or the older `var self`) in the program."""
    names = set()
    for head in tree.find_data("fn_head"):
        params = head.children[2]
        if params is None:
            continue
        first = params.children[0]
        kind, name = first.children[0], first.children[1]
        if isinstance(kind, Token) and kind.type in ("INOUT", "VAR") and name.value == "self":
            names.add(head.children[0].value)
    return names


def violations(variant: str, source: str) -> list[str]:
    """Mutability errors in a program that already parses, in source order."""
    if not source.endswith("\n"):
        source += "\n"
    tree = parser(variant).parse(source)
    checker = Checker(set(MUTATING_METHODS) | mutating_methods(tree))
    for fn in tree.find_data("fn_def"):
        checker.function(fn)
    for test in tree.find_data("test_def"):
        checker.block(test.children[-1], Scope())
    return checker.found


COMMON_LEAKS = {
    "def": r"^[ \t]*def\s",
    "class": r"^[ \t]*class\s",
    "raise": r"\braise\b",
    "try": r"^[ \t]*try[ \t]*:",
    "except": r"^[ \t]*except\b",
    "finally": r"^[ \t]*finally[ \t]*:",
    "with": r"^[ \t]*with\s",
    "decorator": r"^[ \t]*@\w",
    "Optional[": r"\bOptional\[",
    "Union[": r"\bUnion\[",
    "typing generic": r"\b(List|Dict|Tuple|Set)\[",
    "builtin generic": r"\b(list|dict|tuple|set)\[",
    "async": r"\b(async|await)\b",
    "yield": r"\byield\b",
    "global": r"\b(global|nonlocal)\b",
    "isinstance": r"\bisinstance\(",
    "dunder": r"\b__\w+__\b",
    "star args": r"[(,]\s*\*{1,2}[A-Za-z_]",
    "import *": r"\bimport\s+\*",
}

A_ONLY_LEAKS = {
    "None": r"\bNone\b",
    "lambda": r"\blambda\b",
    "import": r"^[ \t]*(import|from)\s",
    "case": r"^[ \t]*case\s",
    "is": r"\bis\b",
}

STRINGS_AND_COMMENTS = re.compile(STRING_RE + r"|#[^\n]*")


def leaks(variant: str, source: str) -> list[str]:
    """Names of Python constructs in `source` that `variant` does not have."""
    code = STRINGS_AND_COMMENTS.sub('""', source)
    patterns = COMMON_LEAKS | (A_ONLY_LEAKS if variant == "a" else {})
    return [name for name, pattern in patterns.items() if re.search(pattern, code, re.MULTILINE)]
