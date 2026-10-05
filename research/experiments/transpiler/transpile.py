"""lotml variants A and B to a Python AST, to execute the pilot's programs and their tests.

Research code for the pilot's subset, not lotml's transpiler. Two modes:

- `lotml` — the language's semantics: value semantics (a mutable value is copied wherever
  it is bound or passed), conditions that only accept `bool` in variant B, and variant A's
  `or` and `if x:` that test optionals for absence rather than for falsiness.
- `python` — the same programs read with Python's semantics: references, truthiness and
  Python's `or`. It is what a model's Python prior predicts the code does.

Types are erased; every node carries its lotml position, so tracebacks show the `.x` line.

Running a program executes it in this process. The programs are model-written, so the
transpiler refuses imports outside `ALLOWED_MODULES` and any dunder name, and the program
sees only `SAFE_BUILTINS`; that narrows what a hostile program can reach but is not a
sandbox. Run new, unreviewed batches in a container or a throwaway machine.
"""

import ast
import builtins
import linecache
import re
import sys
import traceback
import types
from dataclasses import dataclass, field
from functools import cache
from pathlib import Path

import lotml_rt
from lark import Lark, Token, Tree
from lark.exceptions import LarkError

sys.path.insert(0, str(Path(__file__).parent.parent.parent / "pilot"))
from check import GRAMMAR, STRING_RE, VARIANTS, XIndenter

COMPARISONS = {
    "<": ast.Lt,
    ">": ast.Gt,
    "==": ast.Eq,
    ">=": ast.GtE,
    "<=": ast.LtE,
    "!=": ast.NotEq,
    "op_in": ast.In,
    "op_not_in": ast.NotIn,
    "op_is": ast.Is,
    "op_is_not": ast.IsNot,
}
BINARY = {
    "+": ast.Add,
    "-": ast.Sub,
    "*": ast.Mult,
    "/": ast.Div,
    "//": ast.FloorDiv,
    "%": ast.Mod,
}
AUGMENTED = {f"{op}=": cls for op, cls in BINARY.items()}

# The pilot grammar drops anonymous tokens, so `True`/`False` and the comparison
# operators would be indistinguishable in the tree; this copy names them.
COMP_RULE = re.compile(r"^comp_op:.*$", re.MULTILINE)


ALLOWED_MODULES = frozenset({"math"})

# The spec's built-ins, plus the Python constructors and predicates its examples rely on.
SAFE_BUILTINS = frozenset(
    {
        *("print", "len", "range", "enumerate", "zip", "sorted", "reversed", "sum"),
        *("min", "max", "abs", "str", "int", "float", "round", "bool", "list"),
        *("dict", "set", "tuple", "any", "all", "map", "filter"),
    }
)


class TranspileError(Exception):
    """A construct outside the pilot's subset, or one lotml rejects statically."""


def guarded(name: str) -> str:
    if name.startswith("__"):
        raise TranspileError(f"dunder name `{name}` is not lotml")
    return name


def restricted_import(name, globals_=None, locals_=None, fromlist=(), level=0):
    if name not in ALLOWED_MODULES or level != 0:
        raise ImportError(f"import of {name} is outside the research subset")
    return __import__(name, globals_, locals_, fromlist, level)


def program_builtins() -> dict:
    scope = {name: getattr(builtins, name) for name in SAFE_BUILTINS}
    scope["__import__"] = restricted_import
    return scope


@cache
def parser(variant: str) -> Lark:
    grammar = GRAMMAR.format(string_re=STRING_RE, **VARIANTS[variant])
    is_ops = ' | "is" -> op_is | "is" "not" -> op_is_not' if variant == "b" else ""
    grammar = COMP_RULE.sub(
        'comp_op: COMP_OP -> op | "in" -> op_in | "not" "in" -> op_not_in' + is_ops,
        grammar,
    )
    grammar = grammar.replace('"True"', "TRUE").replace('"False"', "FALSE")
    grammar += '\nCOMP_OP: "<=" | ">=" | "==" | "!=" | "<" | ">"\nTRUE: "True"\nFALSE: "False"\n'
    return Lark(grammar, parser="lalr", postlex=XIndenter(), propagate_positions=True)


def rt(name: str) -> ast.Attribute:
    return ast.Attribute(ast.Name("_rt", ast.Load()), name, ast.Load())


def call(function: ast.expr, *args: ast.expr) -> ast.Call:
    return ast.Call(function, list(args), [])


def thunk(body: ast.expr) -> ast.Lambda:
    return ast.Lambda(ast.arguments(), body)


def store(node: ast.expr) -> ast.expr:
    node.ctx = ast.Store()
    if isinstance(node, ast.Tuple):
        for element in node.elts:
            store(element)
    return node


def bare(item):
    """Skip the wrappers the placeholders leave: `test` and `power` with nothing optional."""
    while isinstance(item, Tree) and item.data in ("test", "power"):
        if any(child is not None for child in item.children[1:]):
            break
        item = item.children[0]
    return item


class Transpiler:
    def __init__(self, source: str, variant: str, mode: str):
        self.lines = source.splitlines()
        self.variant = variant
        self.lotml = mode == "lotml"
        self.units: set[str] = set()
        self.fallible = False

    # Positions -------------------------------------------------------------------

    def offset(self, line: int, column: int) -> int:
        """Lark's 1-based character column as `ast`'s 0-based UTF-8 offset."""
        text = self.lines[line - 1] if 0 < line <= len(self.lines) else ""
        return len(text[: column - 1].encode())

    def at[N: ast.AST](self, node: N, item) -> N:
        if isinstance(item, Token):
            span = (item.line, item.column, item.end_line, item.end_column)
        elif isinstance(item, Tree) and not item.meta.empty:
            meta = item.meta
            span = (meta.line, meta.column, meta.end_line, meta.end_column)
        else:
            return node
        if span[0] is None or getattr(node, "lineno", None) is not None:
            return node
        node.lineno, node.col_offset = span[0], self.offset(span[0], span[1])
        node.end_lineno, node.end_col_offset = span[2], self.offset(span[2], span[3])
        return node

    def relocate(self, node: ast.AST, token: Token) -> ast.AST:
        """Move a node parsed from a token's own text to where the token sits."""
        base = self.offset(token.line, token.column)
        for child in ast.walk(node):
            if getattr(child, "lineno", None) is None:
                continue
            if child.lineno == 1:
                child.col_offset += base
            if child.end_lineno == 1:
                child.end_col_offset += base
            child.lineno += token.line - 1
            child.end_lineno += token.line - 1
        return node

    # Semantics ---------------------------------------------------------------------

    def copied(self, node: ast.expr) -> ast.expr:
        """Under value semantics, a value read from a place is copied before it is bound."""
        if self.lotml and isinstance(node, (ast.Name, ast.Attribute, ast.Subscript)):
            return ast.copy_location(call(rt("value"), node), node)
        return node

    def condition(self, node: ast.expr) -> ast.expr:
        if not self.lotml:
            return node
        check = "truth_a" if self.variant == "a" else "condition"
        return ast.copy_location(call(rt(check), node), node)

    # Expressions -------------------------------------------------------------------

    def expr(self, item) -> ast.expr:
        if isinstance(item, Token):
            return self.at(self.token(item), item)
        handler = getattr(self, f"x_{item.data}", None)
        if handler is None:
            raise TranspileError(f"unsupported expression `{item.data}`")
        return self.at(handler(*item.children), item)

    def token(self, token: Token) -> ast.expr:
        if token.type == "NAME":
            return ast.Name(guarded(token.value), ast.Load())
        if token.type == "NUMBER":
            text = token.value.replace("_", "")
            number = float(text) if any(c in text for c in ".eE") else int(text)
            return ast.Constant(number)
        if token.type == "STRING":
            return self.string(token.value, token)
        if token.type in ("TRUE", "FALSE"):
            return ast.Constant(token.type == "TRUE")
        if token.type == "NONE":
            return ast.Constant(None)
        raise TranspileError(f"unexpected token {token.type}")

    def string(self, text: str, token: Token) -> ast.expr:
        try:
            node = ast.parse(text, mode="eval").body
        except SyntaxError as error:
            raise TranspileError(
                f"string literal Python cannot read: {error.msg}"
            ) from None
        for inner in ast.walk(node):
            if isinstance(inner, ast.Name):
                guarded(inner.id)
            elif isinstance(inner, ast.Attribute):
                guarded(inner.attr)
        return self.relocate(node, token)

    def x_atom(self, *strings: Token) -> ast.expr:
        return self.string(" ".join(s.value for s in strings), strings[0])

    def x_test(self, value, test=None, orelse=None) -> ast.expr:
        if test is None:
            return self.expr(value)
        return ast.IfExp(
            self.condition(self.expr(test)), self.expr(value), self.expr(orelse)
        )

    def x_power(self, base, power=None, exponent=None) -> ast.expr:
        if power is None:
            return self.expr(base)
        return ast.BinOp(self.expr(base), ast.Pow(), self.expr(exponent))

    def x_coalesce(self, first, *rest) -> ast.expr:
        node = self.expr(first)
        for item in rest:
            node = call(rt("coalesce"), node, thunk(self.expr(item)))
        return node

    def x_or_test(self, *operands) -> ast.expr:
        if self.lotml and self.variant == "a":
            node = self.expr(operands[0])
            for item in operands[1:]:
                node = call(rt("or_a"), node, thunk(self.expr(item)))
            return node
        return ast.BoolOp(ast.Or(), [self.condition(self.expr(o)) for o in operands])

    def x_and_test(self, *operands) -> ast.expr:
        return ast.BoolOp(ast.And(), [self.condition(self.expr(o)) for o in operands])

    def x_not_op(self, operand) -> ast.expr:
        return ast.UnaryOp(ast.Not(), self.condition(self.expr(operand)))

    def comparand(self, item) -> ast.expr:
        """Variant A writes an expected error as `== fail E`: the value `Err(E)`."""
        inner = bare(item)
        if (
            self.variant == "a"
            and isinstance(inner, Tree)
            and inner.data == "fail_expr"
        ):
            return self.at(
                call(ast.Name("Err", ast.Load()), self.expr(inner.children[0])), inner
            )
        return self.expr(item)

    def x_comparison(self, first, *rest) -> ast.expr:
        ops, comparators = [], []
        for op, item in zip(rest[::2], rest[1::2], strict=True):
            key = op.children[0].value if op.data == "op" else op.data
            ops.append(COMPARISONS[key]())
            comparators.append(self.comparand(item))
        return ast.Compare(self.comparand(first), ops, comparators)

    def binary(self, first, *rest) -> ast.expr:
        node = self.expr(first)
        for op, item in zip(rest[::2], rest[1::2], strict=True):
            node = ast.BinOp(node, BINARY[op.value](), self.expr(item))
        return node

    x_arith = x_term = binary

    def x_unary(self, op: Token, operand) -> ast.expr:
        return ast.UnaryOp(
            ast.USub() if op.value == "-" else ast.UAdd(), self.expr(operand)
        )

    def arguments(self, node) -> tuple[list, list]:
        args, keywords = [], []
        for argument in node.children if node is not None else []:
            if argument.data == "kwarg":
                name, value = argument.children
                keywords.append(ast.keyword(name.value, self.copied(self.expr(value))))
            elif argument.children[1] is not None:
                args.append(
                    self.at(
                        self.generator(ast.GeneratorExp, *argument.children), argument
                    )
                )
            else:
                args.append(self.copied(self.expr(argument.children[0])))
        return args, keywords

    def x_call(self, function, arguments=None) -> ast.expr:
        args, keywords = self.arguments(arguments)
        if (
            isinstance(function, Tree)
            and function.data == "getattr"
            and function.children[1].value in lotml_rt.SPECIAL_NAMES
        ):
            receiver, name = function.children
            return ast.Call(
                rt("method"),
                [self.expr(receiver), ast.Constant(name.value), *args],
                keywords,
            )
        return ast.Call(self.expr(function), args, keywords)

    def x_getitem(self, obj, subscript) -> ast.expr:
        if isinstance(subscript, Tree) and subscript.data == "slice":
            low, high = (
                self.expr(c) if c is not None else None for c in subscript.children
            )
            index = self.at(ast.Slice(low, high), subscript)
        else:
            index = self.expr(subscript)
        return ast.Subscript(self.expr(obj), index, ast.Load())

    def x_getattr(self, obj, name: Token) -> ast.expr:
        return ast.Attribute(self.expr(obj), guarded(name.value), ast.Load())

    def x_try_op(self, operand) -> ast.expr:
        return call(rt("unwrap"), self.expr(operand))

    def x_fail_expr(self, operand) -> ast.expr:
        return call(rt("fail"), self.copied(self.expr(operand)))

    def x_unit(self) -> ast.expr:
        return ast.Constant(None)

    def elements(self, items) -> list[ast.expr]:
        return [self.copied(self.expr(i)) for i in items if i is not None]

    def x_tuple(self, *items) -> ast.expr:
        return ast.Tuple(self.elements(items), ast.Load())

    def x_testlist(self, *items) -> ast.expr:
        return ast.Tuple(self.elements(items), ast.Load())

    def x_list(self, *items) -> ast.expr:
        return ast.List(self.elements(items), ast.Load())

    def x_dict(self, *items) -> ast.expr:
        pairs = [item.children for item in items if item is not None]
        return ast.Dict(
            [self.expr(k) for k, _ in pairs],
            [self.copied(self.expr(v)) for _, v in pairs],
        )

    def comprehensions(self, node) -> list[ast.comprehension]:
        targets, iterable, *rest = node.children
        nested = (
            rest.pop()
            if rest
            and isinstance(rest[-1], Tree | None)
            and (rest[-1] is None or rest[-1].data == "comp_for")
            else None
        )
        ifs = [self.condition(self.expr(c)) for c in rest if c is not None]
        generator = ast.comprehension(
            store(self.targets(targets)), self.expr(iterable), ifs, 0
        )
        return [generator, *(self.comprehensions(nested) if nested is not None else [])]

    def generator(self, kind, element, loops):
        return kind(self.expr(element), self.comprehensions(loops))

    def x_genexp(self, element, loops) -> ast.expr:
        return self.generator(ast.GeneratorExp, element, loops)

    def x_listcomp(self, element, loops) -> ast.expr:
        return self.generator(ast.ListComp, element, loops)

    def x_dictcomp(self, item, loops) -> ast.expr:
        key, value = item.children
        return ast.DictComp(
            self.expr(key), self.expr(value), self.comprehensions(loops)
        )

    def x_lambdef(self, *children) -> ast.expr:
        *names, body = children
        params = [ast.arg(n.value) for n in names if n is not None]
        return ast.Lambda(ast.arguments(args=params), self.expr(body))

    def targets(self, node) -> ast.expr:
        names = [ast.Name(n.value, ast.Store()) for n in node.children]
        if len(names) == 1:
            return self.at(names[0], node.children[0])
        return self.at(ast.Tuple(names, ast.Store()), node)

    # Patterns ----------------------------------------------------------------------

    def pattern(self, item) -> ast.pattern:
        return self.at(getattr(self, f"p_{item.data}")(*item.children), item)

    def p_variant_pattern(self, name: Token, *subpatterns) -> ast.pattern:
        patterns = [self.pattern(p) for p in subpatterns if p is not None]
        return ast.MatchClass(ast.Name(name.value, ast.Load()), patterns, [], [])

    def p_name_pattern(self, name: Token) -> ast.pattern:
        if name.value == "_":
            return ast.MatchAs()
        if name.value in self.units:
            value = ast.Attribute(
                ast.Name("_variants", ast.Load()), name.value, ast.Load()
            )
            return ast.MatchValue(self.at(value, name))
        return ast.MatchAs(name=name.value)

    def p_literal_pattern(self, literal) -> ast.pattern:
        children = [c for c in literal.children if c is not None]
        if children[0].type in ("TRUE", "FALSE", "NONE"):
            return ast.MatchSingleton(self.token(children[0]).value)
        value = self.at(self.token(children[-1]), children[-1])
        if children[0].type == "ADD_OP" and children[0].value == "-":
            value = self.at(ast.UnaryOp(ast.USub(), value), literal)
        return ast.MatchValue(value)

    # Statements --------------------------------------------------------------------

    def stmts(self, item) -> list[ast.stmt]:
        handler = getattr(self, f"s_{item.data}", None)
        if handler is None:
            raise TranspileError(f"unsupported statement `{item.data}`")
        return [self.at(s, item) for s in handler(*item.children)]

    def suite(self, node) -> list[ast.stmt]:
        return [s for child in node.children for s in self.stmts(child)]

    def s_simple_stmt(self, statement):
        return self.stmts(statement)

    def assignable(self, item) -> ast.expr:
        if isinstance(item, Tree) and item.data == "testlist":
            elements = [self.expr(i) for i in item.children]
            return store(self.at(ast.Tuple(elements, ast.Load()), item))
        return store(self.expr(item))

    def s_expr_stmt(self, target, rest=None):
        if rest is None:
            return [ast.Expr(self.expr(target))]
        if rest.data == "augassign":
            op, value = rest.children
            return [
                ast.AugAssign(
                    self.assignable(target), AUGMENTED[op.value](), self.expr(value)
                )
            ]
        value = rest.children[-1]
        return [ast.Assign([self.assignable(target)], self.copied(self.expr(value)))]

    def s_var_stmt(self, _var, name: Token, _type, value):
        target = self.at(ast.Name(name.value, ast.Store()), name)
        return [ast.Assign([target], self.copied(self.expr(value)))]

    def s_return_stmt(self, value=None):
        node = self.expr(value) if value is not None else ast.Constant(None)
        if self.fallible:
            node = ast.copy_location(call(ast.Name("Ok", ast.Load()), node), node)
        return [ast.Return(node)]

    def s_assert_stmt(self, test, message=None):
        msg = self.expr(message) if message is not None else None
        return [ast.Assert(self.condition(self.expr(test)), msg)]

    def s_pass_stmt(self):
        return [ast.Pass()]

    def s_break_stmt(self):
        return [ast.Break()]

    def s_continue_stmt(self):
        return [ast.Continue()]

    def s_if_stmt(self, test, suite, *clauses):
        orelse: list[ast.stmt] = []
        for clause in reversed([c for c in clauses if c is not None]):
            if clause.data == "else_clause":
                orelse = self.suite(clause.children[0])
            else:
                cond, body = clause.children
                orelse = [
                    self.at(
                        ast.If(
                            self.condition(self.expr(cond)), self.suite(body), orelse
                        ),
                        clause,
                    )
                ]
        return [ast.If(self.condition(self.expr(test)), self.suite(suite), orelse)]

    def s_while_stmt(self, test, suite):
        return [ast.While(self.condition(self.expr(test)), self.suite(suite), [])]

    def s_for_stmt(self, targets, iterable, suite):
        return [
            ast.For(self.targets(targets), self.expr(iterable), self.suite(suite), [])
        ]

    def s_match_stmt(self, subject, *arms):
        cases = [
            self.at(
                ast.match_case(
                    self.pattern(arm.children[0]), None, self.suite(arm.children[1])
                ),
                arm,
            )
            for arm in arms
        ]
        if not any(
            isinstance(c.pattern, ast.MatchAs) and c.pattern.pattern is None
            for c in cases
        ):
            unmatched = ast.MatchAs(name="_unmatched")
            fallback = ast.Expr(
                call(rt("no_match"), ast.Name("_unmatched", ast.Load()))
            )
            cases.append(ast.match_case(unmatched, None, [fallback]))
        return [ast.Match(self.expr(subject), cases)]

    # Declarations ------------------------------------------------------------------

    def function(self, node, name: str) -> ast.FunctionDef:
        head, suite = node.children
        _name, _type_params, params, ret_type = head.children
        args, defaults = [], []
        for param in params.children if params is not None else []:
            _var, pname, _ptype, default = param.children
            args.append(self.at(ast.arg(pname.value), pname))
            if default is not None:
                defaults.append(self.expr(default))
        self.fallible = ret_type is not None and ret_type.children[1] is not None
        body = self.suite(suite)
        if self.fallible:
            failure = ast.Attribute(
                ast.Name("_failure", ast.Load()), "error", ast.Load()
            )
            handler = ast.ExceptHandler(
                rt("Fail"),
                "_failure",
                [ast.Return(call(ast.Name("Err", ast.Load()), failure))],
            )
            ok_none = ast.Return(call(ast.Name("Ok", ast.Load()), ast.Constant(None)))
            body = [ast.Try([*body, ok_none], [handler], [], [])]
        self.fallible = False
        function = ast.FunctionDef(
            name,
            ast.arguments(args=args, defaults=defaults),
            body,
            [],
            None,
            type_params=[],
        )
        return self.at(function, node)

    def record(self, name: Token, fields) -> ast.stmt:
        names, defaults = [], {}
        for field_node in fields.children if fields is not None else []:
            fname, _ftype, default = field_node.children
            names.append(ast.Constant(fname.value))
            if default is not None:
                defaults[fname.value] = thunk(self.expr(default))
        builder = call(
            rt("record"),
            ast.Constant(name.value),
            ast.Tuple(names, ast.Load()),
            ast.Dict([ast.Constant(k) for k in defaults], list(defaults.values())),
        )
        return ast.Assign([self.at(ast.Name(name.value, ast.Store()), name)], builder)

    def declarations(self, item) -> list[ast.stmt]:
        if item.data == "record_def":
            name, _params, fields = item.children
            return [self.record(name, fields)]
        statements = []
        for variant in item.children[2:]:
            name, fields = variant.children
            if fields is not None:
                statements.append(self.at(self.record(name, fields), variant))
                continue
            unit = ast.Assign(
                [ast.Name(name.value, ast.Store())],
                call(rt("Unit"), ast.Constant(name.value)),
            )
            register = ast.Assign(
                [
                    ast.Attribute(
                        ast.Name("_variants", ast.Load()), name.value, ast.Store()
                    )
                ],
                ast.Name(name.value, ast.Load()),
            )
            statements += [self.at(unit, variant), self.at(register, variant)]
        return statements

    def implementation(self, item) -> list[ast.stmt]:
        first, second, *functions = item.children
        target = (second or first).children[0].children[0].value
        statements = []
        for node in functions:
            name = node.children[0].children[0].value
            function = self.function(node, f"_{target}_{name}")
            params = node.children[0].children[2]
            is_method = (
                params is not None and params.children[0].children[1].value == "self"
            )
            attach = call(
                rt("attach"),
                ast.Name(target, ast.Load()),
                ast.Constant(name),
                ast.Name(function.name, ast.Load()),
                ast.Constant(is_method),
            )
            statements += [function, self.at(ast.Expr(attach), node)]
        return statements

    def imports(self, item) -> list[ast.stmt]:
        statements = self.import_statements(item)
        for statement in statements:
            names = (
                [statement.module]
                if isinstance(statement, ast.ImportFrom)
                else [alias.name for alias in statement.names]
            )
            for module in names:
                if module not in ALLOWED_MODULES:
                    raise TranspileError(
                        f"import of {module} is outside the research subset"
                    )
        return statements

    def import_statements(self, item) -> list[ast.stmt]:
        if self.variant == "b":
            path, *names = item.children
            module = ".".join(n.value for n in path.children)
            if not names:
                return [ast.Import([ast.alias(module)])]
            return [ast.ImportFrom(module, [ast.alias(n.value) for n in names], 0)]
        parts = [[n.value for n in part.children] for part in item.children]
        prefix = [p[0] for p in parts[:-1]]
        module, names = (".".join(prefix), parts[-1]) if prefix else (None, None)
        if module is None:
            return [ast.Import([ast.alias(parts[-1][0])])]
        return [ast.ImportFrom(module, [ast.alias(n) for n in names], 0)]

    def module(self, tree: Tree) -> ast.Module:
        items = [i for i in tree.children if isinstance(i, Tree)]
        for item in items:
            if item.data == "sum_def":
                self.units |= {
                    v.children[0].value
                    for v in item.children[2:]
                    if v.children[1] is None
                }
        sections: dict[str, list[ast.stmt]] = {
            k: [] for k in ("import", "type", "fn", "impl", "test")
        }
        tests = 0
        for item in items:
            if item.data == "import_stmt":
                sections["import"] += self.imports(item)
            elif item.data in ("record_def", "sum_def"):
                sections["type"] += self.declarations(item)
            elif item.data == "fn_def":
                sections["fn"].append(
                    self.function(item, item.children[0].children[0].value)
                )
            elif item.data == "impl_def":
                sections["impl"] += self.implementation(item)
            elif item.data == "test_def":
                name, suite = item.children
                tests += 1
                function = ast.FunctionDef(
                    f"_test_{tests}",
                    ast.arguments(),
                    self.suite(suite),
                    [],
                    None,
                    type_params=[],
                )
                register = call(
                    ast.Attribute(ast.Name("_tests", ast.Load()), "append", ast.Load()),
                    ast.Tuple(
                        [
                            ast.Constant(ast.literal_eval(name.value)),
                            ast.Name(function.name, ast.Load()),
                        ],
                        ast.Load(),
                    ),
                )
                sections["test"] += [
                    self.at(function, item),
                    self.at(ast.Expr(register), item),
                ]
            elif item.data != "trait_def":
                raise TranspileError(f"unsupported item `{item.data}`")
        body = [s for section in sections.values() for s in section]
        return ast.fix_missing_locations(ast.Module(body, type_ignores=[]))


def transpile(source: str, variant: str, mode: str = "lotml") -> ast.Module:
    if not source.endswith("\n"):
        source += "\n"
    tree = parser(variant).parse(source)
    return Transpiler(source, variant, mode).module(tree)


class StepBudgetExceeded(Exception):
    """A test ran more lines than its budget: an infinite loop, for the experiments."""


def budgeted(budget: int):
    """A trace function that raises once `budget` lines have run."""
    remaining = budget

    def trace(_frame, event, _arg):
        nonlocal remaining
        if event == "line":
            remaining -= 1
            if remaining < 0:
                raise StepBudgetExceeded
        return trace

    return trace


OUTCOMES = [
    (StepBudgetExceeded, "timeout"),
    (lotml_rt.Fail, "propagated error"),
    (lotml_rt.NonExhaustiveMatch, "non-exhaustive match"),
    (lotml_rt.LotmlTypeError, "type error"),
    (AssertionError, "assertion"),
    (NameError, "unresolved name"),
    (Exception, "runtime error"),
]


@dataclass
class Result:
    error: str | None = None
    tests: dict[str, str] = field(default_factory=dict)
    tracebacks: dict[str, str] = field(default_factory=dict)


def run(
    source: str,
    variant: str,
    path: str = "<lotml>",
    mode: str = "lotml",
    budget: int | None = None,
) -> Result:
    """Transpile `source`, load it as `path`, and run every `test` block.

    With a `budget`, a test that runs more than that many lines is stopped as a timeout.
    """
    try:
        tree = transpile(source, variant, mode)
    except (LarkError, TranspileError) as error:
        return Result(error=f"transpile: {str(error).strip().splitlines()[0]}")
    linecache.cache[path] = (len(source), None, source.splitlines(True), path)
    namespace = types.ModuleType("lotml_program").__dict__
    namespace.update(
        _rt=lotml_rt,
        _variants=lotml_rt.Variants(),
        _tests=[],
        Ok=lotml_rt.Ok,
        Err=lotml_rt.Err,
        Heap=lotml_rt.Heap,
        __builtins__=program_builtins(),
    )
    try:
        exec(compile(tree, path, "exec"), namespace)  # noqa: S102
    except Exception as error:  # noqa: BLE001
        return Result(error=f"load: {type(error).__name__}: {error}")
    result = Result()
    for name, test in namespace["_tests"]:
        try:
            if budget is not None:
                sys.settrace(budgeted(budget))
            try:
                test()
            finally:
                sys.settrace(None)
        except Exception as error:  # noqa: BLE001
            kind = next(label for cls, label in OUTCOMES if isinstance(error, cls))
            result.tests[name] = kind
            failure = traceback.TracebackException.from_exception(error)
            frames = [f for f in failure.stack if f.filename == path]
            result.tracebacks[name] = "".join(traceback.format_list(frames)) + "".join(
                failure.format_exception_only()
            )
        else:
            result.tests[name] = "pass"
    return result
