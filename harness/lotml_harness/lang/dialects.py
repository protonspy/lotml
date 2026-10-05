"""lotml's grammar in three published dialects, generated from the parser's single source.

- **EBNF** (W3C notation) at the level of tokens, with its own lexer: `NEWLINE`, `INDENT` and
  `DEDENT` are produced as Python's tokenizer produces them, and `ignored` is skipped
  between tokens. `lark_from_ebnf` reads it back into a parser, which is how it is tested.
- **llguidance Lark** and **GBNF** at the level of characters, for constrained decoding:
  blocks are line-oriented and bounded in depth — each statement line starts with exactly
  four spaces per level, up to `DEPTH` levels — because neither engine has an indenter, and
  whitespace between tokens is explicit, required only between two words.

Every rule of the source grammar maps to rules here; only the tree-shaping marks of Lark
(`?rule`, `-> alias`) are dropped, since they change the tree and not the language.
"""

import re
import re._constants as sre
import re._parser as sre_parse
from dataclasses import dataclass
from functools import cache
from pathlib import Path

from lark import Lark
from lark.grammar import NonTerminal, Terminal
from lark.lexer import Token
from lark.load_grammar import load_grammar

from lotml_harness import ROOT
from lotml_harness.lang.grammar import LotmlIndenter, source

DEPTH = 8
"""Deepest block level the constrained dialects accept; the paired corpus goes 4 deep."""
PUBLISHED = ROOT / "reference" / "grammar"
LAYOUT = {"_NEWLINE": "NEWLINE", "_INDENT": "INDENT", "_DEDENT": "DEDENT"}
BRACKETS = {"(": ")", "[": "]", "{": "}"}
HEADER = "Generated from harness/lotml_harness/lang/grammar.py; do not edit by hand."


class Unportable(ValueError):
    """A grammar or regular-expression feature one of the target engines cannot express."""


# The grammar as data ----------------------------------------------------------------


@dataclass(frozen=True)
class Sym:
    name: str
    terminal: bool


@dataclass(frozen=True)
class Lit:
    text: str


@dataclass(frozen=True)
class Re:
    pattern: str


@dataclass(frozen=True)
class Seq:
    items: tuple


@dataclass(frozen=True)
class Alt:
    options: tuple


@dataclass(frozen=True)
class Rep:
    item: object
    low: int
    high: int | None


EMPTY = Seq(())
MARKS = {(0, 1): "?", (0, None): "*", (1, None): "+"}


def children(node) -> tuple:
    if isinstance(node, Seq):
        return node.items
    if isinstance(node, Alt):
        return node.options
    if isinstance(node, Rep):
        return (node.item,)
    return ()


def seq(items: list):
    items = [i for i in items if i != EMPTY]
    return items[0] if len(items) == 1 else Seq(tuple(items))


def alt(options: list):
    options = list(dict.fromkeys(options))
    return options[0] if len(options) == 1 else Alt(tuple(options))


@dataclass
class Grammar:
    rules: dict[str, object]
    terminals: dict[str, object]
    ignore: list[str]


def unquote(text: str) -> str:
    """A Lark string literal's value."""
    escapes = {"n": "\n", "t": "\t", "r": "\r", "f": "\f"}
    return re.sub(r"\\(.)", lambda m: escapes.get(m[1], m[1]), text[1:-1])


def convert(node) -> object:
    """Lark's parsed grammar notation as this module's nodes."""
    if isinstance(node, NonTerminal):
        return Sym(node.name, terminal=False)
    if isinstance(node, Terminal):
        return Sym(node.name, terminal=True)
    if isinstance(node, Token):
        if node.type == "STRING":
            return Lit(unquote(node.value))
        pattern, _, flags = node.value[1:].rpartition("/")
        if flags:
            raise Unportable(f"regular-expression flags `{flags}`")
        return Re(pattern)
    kind = node.data
    if kind == "expansions":
        return alt([convert(c) for c in node.children])
    if kind == "expansion":
        return seq([convert(c) for c in node.children])
    if kind in ("alias", "value", "literal"):
        return convert(node.children[0])
    if kind == "maybe":
        return Rep(convert(node.children[0]), 0, 1)
    if kind == "expr" and node.children[1].value in "?*+":
        low, high = {"?": (0, 1), "*": (0, None), "+": (1, None)}[node.children[1].value]
        return Rep(convert(node.children[0]), low, high)
    raise Unportable(f"grammar construct `{kind}`")


@cache
def load(variant: str) -> Grammar:
    grammar, _ = load_grammar(source(variant), "<lotml>", [], False)
    rules = {str(name): convert(tree) for name, _params, tree, _options in grammar.rule_defs}
    terminals = {}
    for name, (tree, priority) in grammar.term_defs:
        if tree is None:
            continue
        if priority != 0:
            raise Unportable(f"terminal priority on {name}")
        terminals[name] = convert(tree)
    return Grammar(rules, terminals, list(grammar.ignore))


CONTROL = {"\n": "\\n", "\r": "\\r", "\t": "\\t", "\f": "\\f", "\v": "\\v"}


def escape(text: str) -> str:
    """`text` as a regular expression, with no raw control character or slash in it."""
    return "".join(CONTROL.get(c, "\\/" if c == "/" else re.escape(c)) for c in text)


def slashed(pattern: str) -> str:
    """A pattern safe between Lark's `/…/` delimiters."""
    return re.sub(r"(?<!\\)((?:\\\\)*)/", r"\1\\/", pattern)


def terminal_regex(node, terminals: dict[str, object]) -> str:
    """A terminal's definition as one regular expression, references inlined."""
    if isinstance(node, Lit):
        return escape(node.text)
    if isinstance(node, Re):
        return slashed(node.pattern)
    if isinstance(node, Sym):
        return f"(?:{terminal_regex(terminals[node.name], terminals)})"
    if isinstance(node, Seq):
        return "".join(terminal_regex(i, terminals) for i in node.items)
    if isinstance(node, Alt):
        return "(?:" + "|".join(terminal_regex(o, terminals) for o in node.options) + ")"
    mark = MARKS.get((node.low, node.high), f"{{{node.low},{node.high or ''}}}")
    return f"(?:{terminal_regex(node.item, terminals)}){mark}"


# Regular expressions to grammar notation -------------------------------------------

CATEGORIES = {
    sre.CATEGORY_DIGIT: [("0", "9")],
    sre.CATEGORY_WORD: [("A", "Z"), ("a", "z"), ("0", "9"), ("_", "_")],
    sre.CATEGORY_SPACE: [(c, c) for c in " \t\n\r\f\v"],
}


def parse_regex(pattern: str):
    try:
        return sre_parse.parse(pattern)
    except re.error as error:
        raise Unportable(f"`{pattern}`: {error}") from None


def class_ranges(items) -> tuple[bool, list[tuple[str, str]]]:
    """A character class as (negated, [(low, high)]), or `Unportable`."""
    negated, ranges = False, []
    for op, value in items:
        if op is sre.NEGATE:
            negated = True
        elif op is sre.LITERAL:
            ranges.append((chr(value), chr(value)))
        elif op is sre.RANGE:
            ranges.append((chr(value[0]), chr(value[1])))
        elif op is sre.CATEGORY and value in CATEGORIES:
            ranges += CATEGORIES[value]
        else:
            raise Unportable(f"character class item {op}")
    return negated, ranges


class Notation:
    """How one dialect writes literals, classes, groups and repetition."""

    def literal(self, text: str) -> str:
        raise NotImplementedError

    def char_class(self, negated: bool, ranges: list[tuple[str, str]]) -> str:
        raise NotImplementedError

    def group(self, parts: list[str]) -> str:
        return "(" + " | ".join(parts) + ")"

    def repeat(self, text: str, low: int, high: int | None, atomic: bool) -> str:
        inner = text if atomic else f"({text})"
        return inner + MARKS.get((low, high), f"{{{low},{'' if high is None else high}}}")

    def regex(self, pattern: str) -> str:
        return self.sequence(parse_regex(pattern))

    def sequence(self, sub) -> str:
        parts: list[str] = []
        pending = ""
        for op, value in sub:
            if op is sre.LITERAL:
                pending += chr(value)
                continue
            if pending:
                parts.append(self.literal(pending))
                pending = ""
            parts.append(self.item(op, value))
        if pending:
            parts.append(self.literal(pending))
        return " ".join(parts)

    def atom(self, op, value) -> str:
        if op is sre.LITERAL:
            return self.literal(chr(value))
        if op is sre.NOT_LITERAL:
            return self.char_class(True, [(chr(value), chr(value))])
        if op is sre.ANY:
            return self.char_class(True, [("\n", "\n")])
        if op is sre.IN:
            return self.char_class(*class_ranges(value))
        if op is sre.CATEGORY and value in CATEGORIES:
            return self.char_class(False, CATEGORIES[value])
        if op is sre.BRANCH:
            return self.group([self.sequence(b) for b in value[1]])
        if op is sre.SUBPATTERN:
            _group, add_flags, del_flags, inner = value
            if add_flags or del_flags:
                raise Unportable("inline flags")
            if len(inner) == 1 and inner[0][0] is sre.BRANCH:
                return self.atom(*inner[0])
            return self.group([self.sequence(inner)])
        raise Unportable(f"regular-expression feature {op}")

    def item(self, op, value) -> str:
        if op is sre.MAX_REPEAT:
            low, high, inner = value
            high = None if high is sre.MAXREPEAT else high
            if len(inner) == 1:
                return self.repeat(self.atom(*inner[0]), low, high, atomic=True)
            return self.repeat(self.sequence(inner), low, high, atomic=False)
        if op in (sre.MIN_REPEAT, sre.POSSESSIVE_REPEAT):
            raise Unportable("lazy or possessive repetition")
        return self.atom(op, value)


GBNF_CLASS_ESCAPES = {
    "\n": "\\n",
    "\t": "\\t",
    "\r": "\\r",
    "\f": "\\x0C",
    "\v": "\\x0B",
    "\\": "\\x5C",
    "]": "\\x5D",
    "[": "\\x5B",
    "^": "\\x5E",
    "-": "\\x2D",
}
"""llama.cpp reads `\\xHH` inside a class but not `\\-` or `\\^`."""


class GbnfNotation(Notation):
    def literal(self, text: str) -> str:
        escapes = {"\n": "\\n", "\t": "\\t", "\r": "\\r", '"': '\\"', "\\": "\\\\", "\f": "\\x0C"}
        return '"' + "".join(escapes.get(c, c) for c in text) + '"'

    def char_class(self, negated: bool, ranges: list[tuple[str, str]]) -> str:
        def char(c: str) -> str:
            return GBNF_CLASS_ESCAPES.get(c, c)

        body = "".join(char(a) if a == b else f"{char(a)}-{char(b)}" for a, b in ranges)
        return f"[{'^' if negated else ''}{body}]"


class EbnfNotation(Notation):
    def literal(self, text: str) -> str:
        if any(ord(c) < 32 for c in text):
            return " ".join(f"#x{ord(c):X}" if ord(c) < 32 else self.literal(c) for c in text)
        if '"' not in text:
            return f'"{text}"'
        if "'" not in text:
            return f"'{text}'"
        return " ".join(self.literal(c) for c in text)

    def char_class(self, negated: bool, ranges: list[tuple[str, str]]) -> str:
        def char(c: str) -> str:
            return f"#x{ord(c):X}" if ord(c) < 33 or c in "[]^-\\#'\"" else c

        body = "".join(char(a) if a == b else f"{char(a)}-{char(b)}" for a, b in ranges)
        return f"[{'^' if negated else ''}{body}]"

    def group(self, parts: list[str]) -> str:
        return "( " + " | ".join(parts) + " )"

    def repeat(self, text: str, low: int, high: int | None, atomic: bool) -> str:
        inner = text if atomic else f"( {text} )"
        if (low, high) in MARKS:
            return inner + MARKS[(low, high)]
        if high is None:
            return " ".join([*[inner] * low, inner + "*"])
        optional = ""
        for _ in range(high - low):
            optional = f"( {inner} {optional} )?" if optional else f"{inner}?"
        return " ".join([*[inner] * low, optional])


def regex_to_gbnf(pattern: str) -> str:
    return GbnfNotation().regex(pattern)


def regex_to_ebnf(pattern: str) -> str:
    return EbnfNotation().regex(pattern)


def regex_edge(pattern: str, last: bool) -> tuple[bool, bool]:
    """Whether a match may start (or end) with a word character, and whether it may be empty."""

    def is_word(c: str) -> bool:
        return c.isalnum() or c == "_"

    def items(sub) -> tuple[bool, bool]:
        ordered = list(sub)[::-1] if last else list(sub)
        word = False
        for op, value in ordered:
            item_word, item_empty = one(op, value)
            word = word or item_word
            if not item_empty:
                return word, False
        return word, True

    def one(op, value) -> tuple[bool, bool]:
        if op is sre.LITERAL:
            return is_word(chr(value)), False
        if op in (sre.NOT_LITERAL, sre.ANY):
            return True, False
        if op is sre.IN:
            negated, ranges = class_ranges(value)
            hit = any(is_word(chr(c)) for a, b in ranges for c in range(ord(a), ord(b) + 1))
            return negated or hit, False
        if op is sre.CATEGORY:
            return value in (sre.CATEGORY_WORD, sre.CATEGORY_DIGIT), False
        if op is sre.BRANCH:
            results = [items(b) for b in value[1]]
            return any(w for w, _ in results), any(e for _, e in results)
        if op is sre.SUBPATTERN:
            return items(value[3])
        if op in (sre.MAX_REPEAT, sre.MIN_REPEAT, sre.POSSESSIVE_REPEAT):
            word, empty = items(value[2])
            return word, empty or value[0] == 0
        raise Unportable(f"regular-expression feature {op}")

    return items(parse_regex(pattern))


# EBNF --------------------------------------------------------------------------------


def ebnf_expression(node, top: bool = True) -> str:
    notation = EbnfNotation()
    if isinstance(node, Sym):
        return LAYOUT.get(node.name, node.name)
    if isinstance(node, Lit):
        return notation.literal(node.text)
    if isinstance(node, Re):
        text = notation.regex(node.pattern)
        return text if top or " " not in text else f"( {text} )"
    if isinstance(node, Seq):
        return " ".join(ebnf_expression(i, top=False) for i in node.items)
    if isinstance(node, Alt):
        body = " | ".join(ebnf_expression(o) for o in node.options)
        return body if top else f"( {body} )"
    inner = ebnf_expression(node.item, top=False)
    if " " in inner and not (inner.startswith("( ") and inner.endswith(" )")):
        inner = f"( {inner} )"
    return notation.repeat(inner, node.low, node.high, atomic=True)


def ebnf(variant: str = "b") -> str:
    """The grammar in W3C EBNF, at the level of tokens."""
    grammar = load(variant)
    lines = [
        f"/* lotml grammar, variant {variant.upper()}, in W3C EBNF.",
        f"   {HEADER}",
        "",
        "   Tokens are separated by `ignored`. NEWLINE ends a logical line; inside",
        "   brackets, line breaks are ignored. After each NEWLINE the lexer compares the",
        "   next line's indentation (a tab counts as four spaces) with a stack of open",
        "   levels, as Python's tokenizer does: deeper emits INDENT, shallower emits one",
        "   DEDENT per level closed. A keyword is never a NAME. */",
        "",
    ]
    lines += [f"{name} ::= {ebnf_expression(node)}" for name, node in grammar.rules.items()]
    lines += ["", "/* Tokens */", ""]
    for name, node in grammar.terminals.items():
        if not name.startswith("__"):
            lines.append(f"{LAYOUT.get(name, name)} ::= {ebnf_expression(node)}")
    ignored = [
        ebnf_expression(grammar.terminals[name], top=False) if name.startswith("__") else name
        for name in grammar.ignore
    ]
    lines += ["", "/* Skipped between tokens */", "", "ignored ::= " + " | ".join(ignored)]
    return "\n".join(lines) + "\n"


EBNF_TOKEN = re.compile(
    r"""\s*(?:(?P<comment>/\*.*?\*/)|(?P<define>::=)|(?P<name>[A-Za-z_][A-Za-z0-9_]*)"""
    r"""|(?P<string>"[^"]*"|'[^']*')|(?P<cls>\[[^\]]*\])|(?P<code>\#x[0-9A-Fa-f]+)"""
    r"""|(?P<op>[()|?*+]))""",
    re.DOTALL,
)


def ebnf_productions(text: str) -> dict[str, list[tuple[str, str]]]:
    tokens, position = [], 0
    while text[position:].strip():
        match = EBNF_TOKEN.match(text, position)
        if match is None:
            raise Unportable(f"EBNF at {text[position : position + 20]!r}")
        position = match.end()
        if match.lastgroup != "comment":
            tokens.append((match.lastgroup, match[match.lastgroup]))
    productions: dict[str, list[tuple[str, str]]] = {}
    current = None
    for index, token in enumerate(tokens):
        if token[0] == "name" and tokens[index + 1 : index + 2] == [("define", "::=")]:
            current = productions.setdefault(token[1], [])
        elif token[0] != "define":
            current.append(token)
    return productions


def class_char(c: str) -> str:
    """One character inside a regex class, escaped the way Lark's regex literals need."""
    if c in CONTROL:
        return CONTROL[c]
    return "\\" + c if c in "\\]^-/" else c


def ebnf_class_regex(text: str) -> str:
    """An EBNF character class, `[^#x22a-z]`, as a regex class."""
    body = text[1:-1]
    negated = body.startswith("^")
    parts = re.findall(r"#x[0-9A-Fa-f]+|.", body[negated:])
    out = ""
    for index, part in enumerate(parts):
        if part == "-" and 0 < index < len(parts) - 1:
            out += "-"
        else:
            out += class_char(chr(int(part[2:], 16)) if part.startswith("#x") else part)
    return f"[{'^' if negated else ''}{out}]"


def ebnf_parse(tokens: list[tuple[str, str]]):
    """One EBNF production's expression as this module's nodes."""
    position = 0

    def peek() -> tuple[str, str] | None:
        return tokens[position] if position < len(tokens) else None

    def alternatives():
        nonlocal position
        options = [sequence()]
        while peek() == ("op", "|"):
            position += 1
            options.append(sequence())
        return alt(options)

    def sequence():
        items = []
        while peek() is not None and peek() not in (("op", "|"), ("op", ")")):
            items.append(postfix())
        return seq(items)

    def postfix():
        nonlocal position
        node = primary()
        while peek() in (("op", "?"), ("op", "*"), ("op", "+")):
            low, high = {"?": (0, 1), "*": (0, None), "+": (1, None)}[peek()[1]]
            node = Rep(node, low, high)
            position += 1
        return node

    def primary():
        nonlocal position
        kind, value = tokens[position]
        position += 1
        if (kind, value) == ("op", "("):
            node = alternatives()
            position += 1
            return node
        if kind == "name":
            return Sym(value, terminal=value.isupper())
        if kind == "string":
            return Lit(value[1:-1])
        if kind == "code":
            return Lit(chr(int(value[2:], 16)))
        if kind == "cls":
            return Re(ebnf_class_regex(value))
        raise Unportable(f"EBNF token {value!r}")

    return alternatives()


def rename(node, names: dict[str, str]):
    if isinstance(node, Sym):
        return Sym(names.get(node.name, node.name), node.terminal)
    if isinstance(node, Seq):
        return Seq(tuple(rename(i, names) for i in node.items))
    if isinstance(node, Alt):
        return Alt(tuple(rename(o, names) for o in node.options))
    if isinstance(node, Rep):
        return Rep(rename(node.item, names), node.low, node.high)
    return node


def lark_literal(text: str) -> str:
    return '"' + text.replace("\\", "\\\\").replace('"', '\\"') + '"'


def lark_rule(node) -> str:
    if isinstance(node, Sym):
        return node.name
    if isinstance(node, Lit):
        return lark_literal(node.text)
    if isinstance(node, Seq):
        return " ".join(
            f"({lark_rule(i)})" if isinstance(i, Alt) else lark_rule(i) for i in node.items
        )
    if isinstance(node, Alt):
        return " | ".join(lark_rule(o) for o in node.options)
    return f"({lark_rule(node.item)}){MARKS[(node.low, node.high)]}"


def lark_from_ebnf(text: str) -> Lark:
    """A parser built from the EBNF text alone, with the indenter its comment describes."""
    names = {v: k for k, v in LAYOUT.items()}
    productions = {
        names.get(n, n): rename(ebnf_parse(tokens), names)
        for n, tokens in ebnf_productions(text).items()
    }
    terminals = {n: node for n, node in productions.items() if n.lstrip("_").isupper()}
    lines = [
        f"{name}: {lark_rule(node)}"
        for name, node in productions.items()
        if name not in terminals and name != "ignored"
    ]
    for name, node in terminals.items():
        body = lark_literal(node.text) if isinstance(node, Lit) else None
        lines.append(f"{name}: {body or '/' + terminal_regex(node, terminals) + '/'}")
    ignored = productions["ignored"]
    for index, option in enumerate(ignored.options if isinstance(ignored, Alt) else [ignored]):
        lines += [
            f"IGNORED_{index}: /{terminal_regex(option, terminals)}/",
            f"%ignore IGNORED_{index}",
        ]
    lines.append("%declare _INDENT _DEDENT")
    return Lark("\n".join(lines), parser="lalr", postlex=LotmlIndenter())


# Constrained dialects ---------------------------------------------------------------

LINE_GAP, BRACKET_GAP, END_OF_LINE = "@s", "@sn", "@eol"


class Constrained:
    """The grammar rewritten at the level of characters, with bounded indentation.

    A rule that spans lines exists once per block depth (`name@3`); a rule of expressions
    exists once on a line and once inside brackets (`name@nl`), where line breaks are
    whitespace. Gaps between tokens are explicit and required only between two words.
    """

    def __init__(self, variant: str):
        self.grammar = load(variant)
        self.rules = dict(self.grammar.rules)
        self.layout = self.closure()
        self.out: dict[str, object] = {}
        self.pending: list[tuple[str, str, str, int | None]] = []
        self.memo: dict[tuple, object] = {}

    # Analysis ----------------------------------------------------------------------

    def symbols(self, node):
        if isinstance(node, Sym):
            yield node
        for child in children(node):
            yield from self.symbols(child)

    def closure(self) -> set[str]:
        """Rules that reach a line break, an indent or a dedent."""
        found: set[str] = set()
        changed = True
        while changed:
            changed = False
            for name, node in self.rules.items():
                if name not in found and any(
                    (s.terminal and s.name in LAYOUT) or (not s.terminal and s.name in found)
                    for s in self.symbols(node)
                ):
                    found.add(name)
                    changed = True
        return found

    def recursive(self, key: tuple, default, compute):
        if key not in self.memo:
            self.memo[key] = default
            self.memo[key] = compute()
        return self.memo[key]

    def nullable(self, node) -> bool:
        if isinstance(node, Sym):
            if node.terminal:
                return node.name in ("_INDENT", "_DEDENT")
            return self.recursive(
                ("nullable", node.name), False, lambda: self.nullable(self.rules[node.name])
            )
        if isinstance(node, Seq):
            return all(self.nullable(i) for i in node.items)
        if isinstance(node, Alt):
            return any(self.nullable(o) for o in node.options)
        if isinstance(node, Rep):
            return node.low == 0 or self.nullable(node.item)
        return False

    def word(self, node, last: bool) -> bool:
        """Whether `node` may begin (or, with `last`, end) with a word character."""
        if isinstance(node, Lit):
            c = node.text[-1 if last else 0] if node.text else ""
            return c.isalnum() or c == "_"
        if isinstance(node, Sym):
            if node.terminal:
                if node.name in LAYOUT:
                    return False
                regex = terminal_regex(self.grammar.terminals[node.name], self.grammar.terminals)
                return regex_edge(regex, last)[0]
            return self.recursive(
                ("word", node.name, last), False, lambda: self.word(self.rules[node.name], last)
            )
        if isinstance(node, Seq):
            for item in reversed(node.items) if last else node.items:
                if self.word(item, last):
                    return True
                if not self.nullable(item):
                    return False
            return False
        return any(self.word(c, last) for c in children(node))

    def ends(self, node) -> frozenset:
        """How `node` may end: at a line start (True), mid-line (False), or empty (None)."""
        if isinstance(node, Sym):
            if node.terminal:
                return frozenset({node.name in LAYOUT})
            if node.name not in self.layout:
                return frozenset({False})
            return self.recursive(
                ("ends", node.name), frozenset({True}), lambda: self.ends(self.rules[node.name])
            )
        if isinstance(node, Seq):
            result = frozenset({None})
            for item in node.items:
                ending = self.ends(item)
                result = ending if None not in ending else (ending - {None}) | result
            return result
        if isinstance(node, Alt):
            return frozenset().union(*(self.ends(o) for o in node.options))
        if isinstance(node, Rep):
            ending = self.ends(node.item)
            return ending | {None} if node.low == 0 else ending
        return frozenset({False})

    # Rewriting ---------------------------------------------------------------------

    def ref(self, name: str, mode: str, depth: int | None) -> str | None:
        if name in self.layout:
            if depth is None or depth > DEPTH:
                return None
            key = f"{name}@{depth}"
        else:
            key = f"{name}@nl" if mode == "bracket" else name
        if key not in self.out:
            self.out[key] = None
            self.pending.append((key, name, mode, depth))
        return key

    def build(self) -> dict[str, object]:
        self.ref("start", "line", 0)
        while self.pending:
            key, name, mode, depth = self.pending.pop()
            self.out[key] = self.rewrite(self.rules[name], mode, depth, line_start=False)
        dead = {k for k, v in self.out.items() if v is None}
        while dead:
            self.out = {k: self.prune(v, dead) for k, v in self.out.items() if k not in dead}
            dead = {k for k, v in self.out.items() if v is None}
        reached, frontier = set(), ["start@0"]
        while frontier:
            key = frontier.pop()
            if key not in reached:
                reached.add(key)
                frontier += [s.name for s in self.symbols(self.out[key]) if not s.terminal]
        return {k: v for k, v in self.out.items() if k in reached}

    def prune(self, node, dead: set[str]):
        if isinstance(node, Sym):
            return None if not node.terminal and node.name in dead else node
        if isinstance(node, Seq):
            items = [self.prune(i, dead) for i in node.items]
            return None if None in items else Seq(tuple(items))
        if isinstance(node, Alt):
            options = [o for o in (self.prune(o, dead) for o in node.options) if o is not None]
            return alt(options) if options else None
        if isinstance(node, Rep):
            item = self.prune(node.item, dead)
            if item is None:
                return EMPTY if node.low == 0 else None
            return Rep(item, node.low, node.high)
        return node

    def gap(self, mode: str, required: bool):
        symbol = Sym(BRACKET_GAP if mode == "bracket" else LINE_GAP, terminal=True)
        return symbol if required else Rep(symbol, 0, 1)

    def indented(self, node, depth: int):
        return seq([Lit("    " * depth), node]) if depth > 0 else node

    def rewrite(self, node, mode: str, depth: int | None, line_start: bool):
        """`node` at a block depth, on a line or inside brackets; None when unreachable."""
        if isinstance(node, Sym) and node.terminal and node.name == "_NEWLINE":
            return Rep(Sym(END_OF_LINE, terminal=True), 1, None)
        if isinstance(node, Sym) and node.terminal:
            return self.indented(node, depth) if line_start else node
        if isinstance(node, Sym):
            key = self.ref(node.name, mode, depth)
            if key is None:
                return None
            symbol = Sym(key, terminal=False)
            return self.indented(symbol, depth) if line_start and depth is not None else symbol
        if isinstance(node, Lit | Re):
            return self.indented(node, depth) if line_start else node
        if isinstance(node, Alt):
            options = [self.rewrite(o, mode, depth, line_start) for o in node.options]
            options = [o for o in options if o is not None]
            return alt(options) if options else None
        if isinstance(node, Rep):
            return self.repeat(node, mode, depth, line_start)
        return self.sequence(node.items, mode, depth, line_start)

    def repeat(self, node: Rep, mode: str, depth: int | None, line_start: bool):
        item = self.rewrite(node.item, mode, depth, line_start)
        if item is None:
            return EMPTY if node.low == 0 else None
        if line_start or True in self.ends(node.item) or node.high == 1:
            if line_start and False in self.ends(node.item):
                raise Unportable("a repeated item that may end mid-line starts a line")
            return Rep(item, node.low, node.high)
        rest = Rep(self.joined(node.item, node.item, mode, depth), 0, None)
        whole = seq([item, rest])
        return whole if node.low >= 1 else Rep(whole, 0, 1)

    def split(self, node) -> tuple[object, object] | None:
        """`node` as the part that starts with a word and the part that starts with a sign,
        when it has both — `in` and `<` in a comparison — so only words need a gap."""
        if isinstance(node, Seq) and node.items and not self.nullable(node.items[0]):
            parts = self.split(node.items[0])
            if parts is None:
                return None
            return tuple(seq([part, *node.items[1:]]) for part in parts)
        if not isinstance(node, Sym) or node.terminal or self.nullable(node):
            return None
        body = self.rules[node.name]
        if not isinstance(body, Alt):
            return None
        words = [o for o in body.options if self.word(o, last=False)]
        signs = [o for o in body.options if not self.word(o, last=False)]
        if not words or not signs:
            return None
        names = (f"{node.name}__word", f"{node.name}__sign")
        for name, options in zip(names, (words, signs), strict=True):
            self.rules.setdefault(name, alt(options))
        return tuple(Sym(name, terminal=False) for name in names)

    def joined(self, previous, node, mode: str, depth: int | None):
        """`node` preceded by the gap it needs after `previous`, which ends mid-line."""
        if not self.word(previous, last=True):
            return seq([self.gap(mode, required=False), self.rewrite(node, mode, depth, False)])
        parts = self.split(node)
        if parts is None:
            required = self.word(node, last=False)
            return seq([self.gap(mode, required), self.rewrite(node, mode, depth, False)])
        words, signs = (self.rewrite(part, mode, depth, False) for part in parts)
        return alt(
            [
                seq([self.gap(mode, required=True), words]),
                seq([self.gap(mode, required=False), signs]),
            ]
        )

    def sequence(self, items, mode: str, depth: int | None, line_start: bool):
        out: list = []
        at_line_start = line_start
        opened = 0
        previous = None
        previous_index = None
        for item in items:
            if isinstance(item, Sym) and item.name in ("_INDENT", "_DEDENT"):
                depth = None if depth is None else depth + (1 if item.name == "_INDENT" else -1)
                continue
            item_mode = "bracket" if mode == "bracket" or opened > 0 else "line"
            layout = isinstance(item, Sym) and item.name == "_NEWLINE"
            rewritten = self.rewrite(item, item_mode, depth, at_line_start and not layout)
            if rewritten is None:
                return None
            if previous is not None and not at_line_start and not layout:
                if isinstance(item, Rep) and item.high != 1 and False in self.ends(item.item):
                    # Every iteration needs its gap: the first after `previous`, the rest
                    # after an iteration, so the wordier of the two decides.
                    before = previous if self.word(previous, last=True) else item.item
                    joined = self.joined(before, item.item, item_mode, depth)
                    rewritten = Rep(joined, item.low, item.high)
                elif self.nullable(item):
                    inner = item.item if isinstance(item, Rep) and item.high == 1 else item
                    rewritten = Rep(self.joined(previous, inner, item_mode, depth), 0, 1)
                elif (
                    self.nullable(previous)
                    and previous_index is not None
                    and self.word(previous, last=True)
                    and self.word(item, last=False)
                ):
                    group = out[previous_index]
                    gap = self.gap(item_mode, required=True)
                    out[previous_index] = Rep(seq([group.item, gap]), group.low, group.high)
                    out.append(self.gap(item_mode, required=False))
                else:
                    rewritten = self.joined(previous, item, item_mode, depth)
            out.append(rewritten)
            previous_index = (
                len(out) - 1 if isinstance(rewritten, Rep) and rewritten.low == 0 else None
            )
            if isinstance(item, Lit) and item.text in BRACKETS:
                opened += 1
            elif isinstance(item, Lit) and item.text in BRACKETS.values():
                opened -= 1
            ending = self.ends(item)
            if None in ending:
                ending = (ending - {None}) | {at_line_start}
            if len(ending) > 1:
                raise Unportable(f"a line may or may not end inside {items!r}")
            at_line_start = True in ending
            previous = item
        return seq(out)


def constrained(variant: str) -> tuple[dict[str, object], dict[str, str]]:
    """The rewritten rules and each terminal they use as one regular expression."""
    writer = Constrained(variant)
    rules = writer.build()
    grammar = writer.grammar
    comment = r"#[^\n]*"
    continuation = r"\\[ \t\f]*\r?\n"
    patterns = {
        name: terminal_regex(node, grammar.terminals)
        for name, node in grammar.terminals.items()
        if not name.startswith("_")
    }
    patterns[LINE_GAP] = rf"([ \t\f]|{continuation})+"
    patterns[BRACKET_GAP] = rf"([ \t\f\r\n]|{continuation}|{comment})+"
    # One line end per lexeme, blank lines being further ones: llguidance's lexer looks one
    # byte ahead, so a lexeme that could run on into the next line's indentation would
    # swallow it.
    patterns[END_OF_LINE] = rf"[ \t\f]*({comment})?\r?\n"
    used: set[str] = set()
    for node in rules.values():
        used |= {s.name for s in writer.symbols(node) if s.terminal}
    return rules, {name: patterns[name] for name in patterns if name in used}


def rule_name(key: str) -> str:
    """`if_stmt@3` → `if_stmt_d3`, `test@nl` → `test_nl`, `_comparison` → `u_comparison`."""
    name, _, suffix = key.partition("@")
    name = "u" + name if name.startswith("_") else name
    return name if not suffix else f"{name}_{'nl' if suffix == 'nl' else 'd' + suffix}"


def terminal_name(name: str) -> str:
    return {LINE_GAP: "WS", BRACKET_GAP: "WSN", END_OF_LINE: "EOL"}.get(name, name)


def lark_expression(node) -> str:
    if isinstance(node, Sym):
        return terminal_name(node.name) if node.terminal else rule_name(node.name)
    if isinstance(node, Lit):
        return lark_literal(node.text)
    if isinstance(node, Seq):
        if not node.items:
            return '""'
        return " ".join(
            f"({lark_expression(i)})" if isinstance(i, Alt) else lark_expression(i)
            for i in node.items
        )
    if isinstance(node, Alt):
        return " | ".join(lark_expression(o) for o in node.options)
    inner = lark_expression(node.item)
    atomic = isinstance(node.item, Sym | Lit)
    return (inner if atomic else f"({inner})") + MARKS[(node.low, node.high)]


def llguidance(variant: str = "b") -> str:
    """The grammar in llguidance's Lark: line-oriented, depth-bounded, explicit whitespace."""
    rules, terminals = constrained(variant)
    lines = [
        f"// lotml grammar, variant {variant.upper()}, for llguidance (constrained decoding).",
        f"// {HEADER}",
        f"// Statements start with four spaces per block level, at most {DEPTH} levels deep.",
        "",
        "start: start_d0",
    ]
    lines += [f"{rule_name(name)}: {lark_expression(node)}" for name, node in rules.items()]
    lines += [f"{terminal_name(name)}: /{pattern}/" for name, pattern in terminals.items()]
    return "\n".join(lines) + "\n"


def gbnf_rule_name(key: str) -> str:
    return rule_name(key).lower().replace("_", "-")


def gbnf_terminal_name(name: str) -> str:
    return "t-" + terminal_name(name).lower().replace("_", "-")


def gbnf_expression(node) -> str:
    notation = GbnfNotation()
    if isinstance(node, Sym):
        return gbnf_terminal_name(node.name) if node.terminal else gbnf_rule_name(node.name)
    if isinstance(node, Lit):
        return notation.literal(node.text)
    if isinstance(node, Seq):
        if not node.items:
            return '""'
        return " ".join(
            f"({gbnf_expression(i)})" if isinstance(i, Alt) else gbnf_expression(i)
            for i in node.items
        )
    if isinstance(node, Alt):
        return " | ".join(gbnf_expression(o) for o in node.options)
    atomic = isinstance(node.item, Sym | Lit)
    return notation.repeat(gbnf_expression(node.item), node.low, node.high, atomic)


def gbnf(variant: str = "b") -> str:
    """The grammar in llama.cpp's GBNF: line-oriented, depth-bounded, explicit whitespace."""
    rules, terminals = constrained(variant)
    lines = [
        f"# lotml grammar, variant {variant.upper()}, in GBNF (constrained decoding).",
        f"# {HEADER}",
        f"# Statements start with four spaces per block level, at most {DEPTH} levels deep.",
        "",
        "root ::= start-d0",
    ]
    lines += [f"{gbnf_rule_name(name)} ::= {gbnf_expression(node)}" for name, node in rules.items()]
    for name, pattern in terminals.items():
        if name in (LINE_GAP, BRACKET_GAP):
            # Whitespace as a recursive rule rather than a run: engines that fuse rules
            # made only of characters into one lexeme would otherwise fuse `is` `not`.
            unit = regex_to_gbnf(pattern.removesuffix("+"))
            lines.append(f"{gbnf_terminal_name(name)} ::= {unit} {gbnf_terminal_name(name)}?")
        else:
            lines.append(f"{gbnf_terminal_name(name)} ::= {regex_to_gbnf(pattern)}")
    return "\n".join(lines) + "\n"


def published() -> dict[Path, str]:
    """Each published grammar file and the text it should hold."""
    return {
        PUBLISHED / "lotml.ebnf": ebnf("b"),
        PUBLISHED / "lotml.lark": llguidance("b"),
        PUBLISHED / "lotml.gbnf": gbnf("b"),
    }


def write() -> None:
    """Regenerate the published grammars from the source."""
    for path, text in published().items():
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(text, encoding="utf-8", newline="\n")


if __name__ == "__main__":
    write()
