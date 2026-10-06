"""lotml, variant A or B, to a Python AST that runs under lotml's semantics or Python's.

- `lotml` mode — the language's semantics: values are copied where they are bound or
  stored, `inout` arguments are written back to the caller, `int` results trap outside
  i64, conditions accept only `bool` (variant A: an optional tests for presence), and
  `match` without a matching arm stops the program.
- `python` mode — the same program read with Python's semantics: references, truthiness,
  unbounded integers. It is what a model's Python prior predicts the code does.

Types are erased. Every node carries its lotml position, so a traceback names the
`.lotml` line. The transpiler refuses imports outside `ALLOWED_MODULES` and every name
starting with `__`, and names everything it generates with that prefix — the runtime, the
variant table, the test list, temporaries — so a program can write none of them; the
program sees only the prelude. That is a language boundary, not an operating-system one:
`execute` adds a child process with a minimal environment and limits, and an unreviewed
batch of model output still belongs on a machine you can throw away.
"""

import ast
from itertools import count

from lark import Token, Tree

from lotml_harness.lang import runtime
from lotml_harness.lang.grammar import parser

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
    "**": ast.Pow,
    "<<": ast.LShift,
    ">>": ast.RShift,
    "|": ast.BitOr,
    "^": ast.BitXor,
    "&": ast.BitAnd,
}
AUGMENTED = {f"{op}=": cls for op, cls in BINARY.items()}
OVERFLOWING = (ast.Add, ast.Sub, ast.Mult, ast.Pow, ast.LShift, ast.FloorDiv)
"""Operators whose `int` result can leave i64."""

ALLOWED_MODULES = frozenset({"math"})


class TranspileError(Exception):
    """A construct the executor cannot run, or one lotml rejects statically."""


def guarded(name: str) -> str:
    if name.startswith("__"):
        raise TranspileError(f"dunder name `{name}` is not lotml")
    return name


def rt(name: str) -> ast.Attribute:
    return ast.Attribute(ast.Name("__rt", ast.Load()), name, ast.Load())


def call(function: ast.expr, *args: ast.expr) -> ast.Call:
    return ast.Call(function, list(args), [])


def thunk(body: ast.expr) -> ast.Lambda:
    return ast.Lambda(ast.arguments(), body)


def store(node: ast.expr) -> ast.expr:
    node.ctx = ast.Store()
    if isinstance(node, ast.Tuple | ast.List):
        for element in node.elts:
            store(element)
    return node


def load(node: ast.expr) -> ast.expr:
    """A copy of an assignment target, read instead of written."""
    copied = ast.parse(ast.unparse(node), mode="eval").body
    return ast.copy_location(copied, node)


def bare(item):
    """Skip the wrappers the placeholders leave: `test` and `power` with nothing optional."""
    while isinstance(item, Tree) and item.data in ("test", "power"):
        if any(child is not None for child in item.children[1:]):
            break
        item = item.children[0]
    return item


def convention(param: Tree) -> str | None:
    token = param.children[0]
    return token.value if isinstance(token, Token) else None


class Transpiler:
    def __init__(self, source: str, variant: str, mode: str):
        self.lines = source.splitlines()
        self.variant = variant
        self.lotml = mode == "lotml"
        self.units: set[str] = set()
        self.types: set[str] = set()
        self.fallible = False
        self.boxed: set[str] = set()
        self.temporaries = count(1)

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
        """Under value semantics, a value bound or stored is a copy of where it came from."""
        if self.lotml and isinstance(
            node, ast.Name | ast.Attribute | ast.Subscript | ast.Call | ast.IfExp
        ):
            return ast.copy_location(call(rt("value"), node), node)
        return node

    def condition(self, node: ast.expr) -> ast.expr:
        if not self.lotml:
            return node
        check = "truth_a" if self.variant == "a" else "condition"
        return ast.copy_location(call(rt(check), node), node)

    def checked(self, node: ast.expr) -> ast.expr:
        """An arithmetic result that traps outside i64 under lotml's semantics."""
        if self.lotml:
            return ast.copy_location(call(rt("i64"), node), node)
        return node

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
            name = ast.Name(guarded(token.value), ast.Load())
            if token.value in self.boxed:
                return ast.Attribute(name, "value", ast.Load())
            return name
        if token.type == "NUMBER":
            return ast.Constant(ast.literal_eval(token.value.replace("_", "")))
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
            raise TranspileError(f"string literal Python cannot read: {error.msg}") from None
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
        return ast.IfExp(self.condition(self.expr(test)), self.expr(value), self.expr(orelse))

    def operation(self, left: ast.expr, operator: ast.operator, right: ast.expr) -> ast.expr:
        """`left operator right` under the target's semantics.

        Under lotml's, `**` and `<<` go through the runtime, which traps a result too wide for
        i64 before computing it — `10 ** 10 ** 9` would otherwise run for hours first.
        """
        if self.lotml and isinstance(operator, ast.Pow):
            return call(rt("power"), left, right)
        if self.lotml and isinstance(operator, ast.LShift):
            return call(rt("lshift"), left, right)
        node = ast.BinOp(left, operator, right)
        return self.checked(node) if isinstance(operator, OVERFLOWING) else node

    def x_power(self, base, _power=None, exponent=None) -> ast.expr:
        if exponent is None:
            return self.expr(base)
        return self.operation(self.expr(base), ast.Pow(), self.expr(exponent))

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
        if self.variant == "a" and isinstance(inner, Tree) and inner.data == "fail_expr":
            return self.at(call(ast.Name("Err", ast.Load()), self.expr(inner.children[0])), inner)
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
            node = self.operation(node, BINARY[op.value](), self.expr(item))
        return node

    x_arith = x_term = x_shift = binary

    def chain(self, operator: type[ast.operator], operands) -> ast.expr:
        node = self.expr(operands[0])
        for item in operands[1:]:
            node = ast.BinOp(node, operator(), self.expr(item))
        return node

    def x_bit_or(self, *operands) -> ast.expr:
        return self.chain(ast.BitOr, operands)

    def x_bit_xor(self, *operands) -> ast.expr:
        return self.chain(ast.BitXor, operands)

    def x_bit_and(self, *operands) -> ast.expr:
        return self.chain(ast.BitAnd, operands)

    def x_unary(self, op: Token, operand) -> ast.expr:
        if op.value == "+":
            return ast.UnaryOp(ast.UAdd(), self.expr(operand))
        return self.checked(ast.UnaryOp(ast.USub(), self.expr(operand)))

    def x_invert(self, operand) -> ast.expr:
        return ast.UnaryOp(ast.Invert(), self.expr(operand))

    def arguments(self, node, store_values: bool) -> tuple[list, list, list]:
        """Positional arguments, keywords, and the write-backs `inout` arguments need."""
        args, keywords, writebacks = [], [], []
        maybe_copied = self.copied if store_values else (lambda n: n)
        for argument in node.children if node is not None else []:
            if argument.data == "kwarg":
                name, value = argument.children
                keywords.append(ast.keyword(guarded(name.value), maybe_copied(self.expr(value))))
            elif argument.data == "inout_arg":
                place = self.expr(argument.children[0])
                if not self.lotml:
                    args.append(place)
                    continue
                box = f"__inout{next(self.temporaries)}"
                args.append(ast.NamedExpr(ast.Name(box, ast.Store()), call(rt("Box"), place)))
                writebacks.append(self.writeback(place, ast.Name(box, ast.Load())))
            elif argument.children[1] is not None:
                args.append(self.at(self.generator(ast.GeneratorExp, *argument.children), argument))
            else:
                args.append(maybe_copied(self.expr(argument.children[0])))
        return args, keywords, writebacks

    def writeback(self, place: ast.expr, box: ast.expr) -> ast.expr:
        """An expression storing the box's final value back into the caller's place."""
        final = ast.Attribute(box, "value", ast.Load())
        if isinstance(place, ast.Name):
            return ast.NamedExpr(ast.Name(place.id, ast.Store()), final)
        if isinstance(place, ast.Attribute):
            return call(rt("set_attr"), place.value, ast.Constant(place.attr), final)
        if isinstance(place, ast.Subscript):
            return call(rt("set_item"), place.value, place.slice, final)
        raise TranspileError("`&` needs a variable, a field or an element")

    def x_call(self, function, arguments=None) -> ast.expr:
        is_method = isinstance(function, Tree) and function.data == "getattr"
        constructs = isinstance(function, Token) and function.value in self.types
        args, keywords, writebacks = self.arguments(arguments, is_method or constructs)
        if is_method and function.children[1].value in runtime.SPECIAL_NAMES:
            receiver, name = function.children
            node = ast.Call(
                rt("method"), [self.expr(receiver), ast.Constant(name.value), *args], keywords
            )
        else:
            node = ast.Call(self.expr(function), args, keywords)
        if writebacks:
            node = call(rt("returning"), node, *writebacks)
        return node

    def x_getitem(self, obj, subscript) -> ast.expr:
        if isinstance(subscript, Tree) and subscript.data == "slice":
            low, high, step = (self.expr(c) if c is not None else None for c in subscript.children)
            index = self.at(ast.Slice(low, high, step), subscript)
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

    def x_set(self, *items) -> ast.expr:
        return ast.Set(self.elements(items))

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
        generator = ast.comprehension(store(self.targets(targets)), self.expr(iterable), ifs, 0)
        return [generator, *(self.comprehensions(nested) if nested is not None else [])]

    def generator(self, kind, element, loops):
        return kind(self.copied(self.expr(element)), self.comprehensions(loops))

    def x_genexp(self, element, loops) -> ast.expr:
        return self.generator(ast.GeneratorExp, element, loops)

    def x_listcomp(self, element, loops) -> ast.expr:
        return self.generator(ast.ListComp, element, loops)

    def x_setcomp(self, element, loops) -> ast.expr:
        return self.generator(ast.SetComp, element, loops)

    def x_dictcomp(self, item, loops) -> ast.expr:
        key, value = item.children
        return ast.DictComp(
            self.expr(key), self.copied(self.expr(value)), self.comprehensions(loops)
        )

    def x_lambdef(self, *children) -> ast.expr:
        *names, body = children
        params = [ast.arg(n.value) for n in names if n is not None]
        return ast.Lambda(ast.arguments(args=params), self.expr(body))

    def targets(self, node) -> ast.expr:
        names = []
        for child in node.children:
            if isinstance(child, Token):
                names.append(self.at(ast.Name(guarded(child.value), ast.Store()), child))
            else:
                names.append(self.targets(child))
        if len(names) == 1:
            return names[0]
        return self.at(ast.Tuple(names, ast.Store()), node)

    # Patterns ----------------------------------------------------------------------

    def pattern(self, item) -> ast.pattern:
        return self.at(getattr(self, f"p_{item.data}")(*item.children), item)

    def p_variant_pattern(self, name: Token, *subpatterns) -> ast.pattern:
        patterns = [self.pattern(p) for p in subpatterns if p is not None]
        return ast.MatchClass(ast.Name(guarded(name.value), ast.Load()), patterns, [], [])

    def p_tuple_pattern(self, *subpatterns) -> ast.pattern:
        return ast.MatchSequence([self.pattern(p) for p in subpatterns if p is not None])

    def p_name_pattern(self, name: Token) -> ast.pattern:
        if name.value == "_":
            return ast.MatchAs()
        if name.value in self.units:
            value = ast.Attribute(ast.Name("__variants", ast.Load()), name.value, ast.Load())
            return ast.MatchValue(self.at(value, name))
        return ast.MatchAs(name=guarded(name.value))

    def p_literal_pattern(self, literal) -> ast.pattern:
        children = [c for c in literal.children if c is not None]
        if children[0].type in ("TRUE", "FALSE", "NONE"):
            return ast.MatchSingleton(self.token(children[0]).value)
        value = self.at(self.token(children[-1]), children[-1])
        if children[0].type == "MINUS":
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
            operator = AUGMENTED[op.value]()
            place = self.assignable(target)
            if self.lotml and isinstance(operator, OVERFLOWING):
                total = self.operation(load(place), operator, self.expr(value))
                return [ast.Assign([place], total)]
            return [ast.AugAssign(place, operator, self.expr(value))]
        value = rest.children[-1]
        return [ast.Assign([self.assignable(target)], self.copied(self.expr(value)))]

    def s_var_stmt(self, _var, name: Token, _type, value):
        target = self.at(ast.Name(guarded(name.value), ast.Store()), name)
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
                        ast.If(self.condition(self.expr(cond)), self.suite(body), orelse),
                        clause,
                    )
                ]
        return [ast.If(self.condition(self.expr(test)), self.suite(suite), orelse)]

    def s_while_stmt(self, test, suite):
        return [ast.While(self.condition(self.expr(test)), self.suite(suite), [])]

    def s_for_stmt(self, targets, iterable, suite):
        return [ast.For(self.targets(targets), self.expr(iterable), self.suite(suite), [])]

    def s_match_stmt(self, subject, *arms):
        cases = [
            self.at(
                ast.match_case(self.pattern(arm.children[-2]), None, self.suite(arm.children[-1])),
                arm,
            )
            for arm in arms
        ]
        if not any(isinstance(c.pattern, ast.MatchAs) and c.pattern.pattern is None for c in cases):
            unmatched = ast.MatchAs(name="__unmatched")
            fallback = ast.Expr(call(rt("no_match"), ast.Name("__unmatched", ast.Load())))
            cases.append(ast.match_case(unmatched, None, [fallback]))
        return [ast.Match(self.expr(subject), cases)]

    # Declarations ------------------------------------------------------------------

    def function(self, node, name: str, generated: bool = False) -> ast.FunctionDef:
        head, suite = node.children
        _name, _type_params, params, ret_type = head.children
        args, defaults, boxed, copies = [], [], set(), []
        for param in params.children if params is not None else []:
            _kind, pname, _ptype, default = param.children
            args.append(self.at(ast.arg(guarded(pname.value)), pname))
            if default is not None:
                defaults.append(self.expr(default))
            if not self.lotml or pname.value == "self":
                continue
            if convention(param) == "inout":
                boxed.add(pname.value)
            elif convention(param) == "var":
                local = ast.Name(pname.value, ast.Store())
                copies.append(ast.Assign([local], call(rt("value"), ast.Name(pname.value))))
        outer_boxed, self.boxed = self.boxed, boxed
        self.fallible = ret_type is not None and ret_type.children[1] is not None
        body = copies + self.suite(suite)
        if self.fallible:
            failure = ast.Attribute(ast.Name("__failure", ast.Load()), "error", ast.Load())
            handler = ast.ExceptHandler(
                rt("Fail"),
                "__failure",
                [ast.Return(call(ast.Name("Err", ast.Load()), failure))],
            )
            ok_none = ast.Return(call(ast.Name("Ok", ast.Load()), ast.Constant(None)))
            body = [ast.Try([*body, ok_none], [handler], [], [])]
        self.fallible = False
        self.boxed = outer_boxed
        function = ast.FunctionDef(
            name if generated else guarded(name),
            ast.arguments(args=args, defaults=defaults),
            body,
            [],
            None,
            type_params=[],
        )
        return self.at(function, node)

    def record(self, name: Token, fields: list[Tree]) -> ast.stmt:
        names, defaults = [], {}
        for index, field_node in enumerate(fields):
            if field_node.data == "field":
                fname, _ftype, default = field_node.children
                names.append(ast.Constant(guarded(fname.value)))
                if default is not None:
                    defaults[fname.value] = thunk(self.expr(default))
            else:
                names.append(ast.Constant(f"_{index}"))
        builder = call(
            rt("record"),
            ast.Constant(name.value),
            ast.Tuple(names, ast.Load()),
            ast.Dict([ast.Constant(k) for k in defaults], list(defaults.values())),
        )
        return ast.Assign([self.at(ast.Name(guarded(name.value), ast.Store()), name)], builder)

    def declarations(self, item) -> list[ast.stmt]:
        if item.data == "record_def":
            name, _params, fields = item.children
            return [self.record(name, fields.children if fields is not None else [])]
        statements = []
        for variant in item.children[2:]:
            name, fields = variant.children
            if fields is not None:
                statements.append(self.at(self.record(name, fields.children), variant))
                continue
            unit = ast.Assign(
                [ast.Name(guarded(name.value), ast.Store())],
                call(rt("Unit"), ast.Constant(name.value)),
            )
            register = ast.Assign(
                [ast.Attribute(ast.Name("__variants", ast.Load()), name.value, ast.Store())],
                ast.Name(name.value, ast.Load()),
            )
            statements += [self.at(unit, variant), self.at(register, variant)]
        return statements

    def method(self, target: str, node: Tree) -> list[ast.stmt]:
        name = node.children[0].children[0].value
        function = self.function(node, f"__{target}_{name}", generated=True)
        params = node.children[0].children[2]
        is_method = params is not None and params.children[0].children[1].value == "self"
        attach = call(
            rt("attach"),
            ast.Name(target, ast.Load()),
            ast.Constant(name),
            ast.Name(function.name, ast.Load()),
            ast.Constant(is_method),
        )
        return [function, self.at(ast.Expr(attach), node)]

    @staticmethod
    def type_name(node: Tree) -> str:
        return node.children[0].children[0].value

    def implementation(self, item, defaults: dict[str, list[str]]) -> list[ast.stmt]:
        first, second, *functions = item.children
        target = self.type_name(second or first)
        statements = [s for node in functions for s in self.method(target, node)]
        trait = self.type_name(first) if second is not None else None
        if trait in defaults:
            table = ast.Dict(
                [ast.Constant(m) for m in defaults[trait]],
                [ast.Name(f"__{trait}_{m}", ast.Load()) for m in defaults[trait]],
            )
            attach = call(rt("attach_defaults"), ast.Name(target, ast.Load()), table)
            statements.append(self.at(ast.Expr(attach), item))
        return statements

    def trait(self, item) -> tuple[list[ast.stmt], list[str]]:
        """A trait's default methods as functions, and their names."""
        name = item.children[0].value
        functions, methods = [], []
        for member in item.children[2:]:
            if isinstance(member, Tree) and member.data == "fn_def":
                method = member.children[0].children[0].value
                functions.append(self.function(member, f"__{name}_{method}", generated=True))
                methods.append(method)
        return functions, methods

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
                    raise TranspileError(f"import of {module} is outside the prelude's modules")
        return statements

    def import_statements(self, item) -> list[ast.stmt]:
        if self.variant == "b":
            path, *names = item.children
            module = ".".join(n.value for n in path.children)
            if not names or names == [None]:
                return [ast.Import([ast.alias(module)])]
            return [ast.ImportFrom(module, [ast.alias(n.value) for n in names], 0)]
        parts = [[n.value for n in part.children] for part in item.children]
        prefix = [p[0] for p in parts[:-1]]
        if not prefix:
            return [ast.Import([ast.alias(parts[-1][0])])]
        return [ast.ImportFrom(".".join(prefix), [ast.alias(n) for n in parts[-1]], 0)]

    def module(self, tree: Tree) -> ast.Module:
        items = [i for i in tree.children if isinstance(i, Tree)]
        for item in items:
            if item.data == "sum_def":
                self.types |= {v.children[0].value for v in item.children[2:]}
                self.units |= {
                    v.children[0].value for v in item.children[2:] if v.children[1] is None
                }
            elif item.data == "record_def":
                self.types.add(item.children[0].value)
        sections: dict[str, list[ast.stmt]] = {
            k: [] for k in ("import", "type", "trait", "fn", "impl", "test")
        }
        defaults: dict[str, list[str]] = {}
        tests = 0
        for item in items:
            if item.data == "import_stmt":
                sections["import"] += self.imports(item)
            elif item.data in ("record_def", "sum_def"):
                sections["type"] += self.declarations(item)
            elif item.data == "trait_def":
                functions, methods = self.trait(item)
                sections["trait"] += functions
                defaults[item.children[0].value] = methods
            elif item.data == "fn_def":
                sections["fn"].append(self.function(item, item.children[0].children[0].value))
            elif item.data == "impl_def":
                sections["impl"].append(item)
            elif item.data == "test_def":
                name, suite = item.children
                tests += 1
                function = ast.FunctionDef(
                    f"__test_{tests}", ast.arguments(), self.suite(suite), [], None, type_params=[]
                )
                register = call(
                    ast.Attribute(ast.Name("__tests", ast.Load()), "append", ast.Load()),
                    ast.Tuple(
                        [
                            ast.Constant(ast.literal_eval(name.value)),
                            ast.Name(function.name, ast.Load()),
                        ],
                        ast.Load(),
                    ),
                )
                sections["test"] += [self.at(function, item), self.at(ast.Expr(register), item)]
            else:
                raise TranspileError(f"unsupported item `{item.data}`")
        sections["impl"] = [s for i in sections["impl"] for s in self.implementation(i, defaults)]
        body = [s for section in sections.values() for s in section]
        return ast.fix_missing_locations(ast.Module(body, type_ignores=[]))


def transpile(source: str, variant: str, mode: str = "lotml") -> ast.Module:
    """The Python module for a lotml program; raises Lark's errors or `TranspileError`."""
    if not source.endswith("\n"):
        source += "\n"
    tree = parser(variant).parse(source)
    return Transpiler(source, variant, mode).module(tree)
