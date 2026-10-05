"""Syntax check and Python-leakage detection for the two pilot variants of the X language."""

import re
from functools import cache
from typing import ClassVar

from lark import Lark
from lark.exceptions import LarkError
from lark.indenter import Indenter

STRING_RE = (
    r"[rRbBfF]{0,2}("
    r'"""(.|\n)*?"""'
    r"|'''(.|\n)*?'''"
    r'|"(?:[^"\\\n]|\\.)*+"'
    r"|'(?:[^'\\\n]|\\.)*+'"
    r")"
)

GRAMMAR = r"""
start: (_NEWLINE | item)*

?item: fn_def | type_def | impl_def | trait_def | import_stmt | test_def

fn_def: fn_head ":" suite
fn_head: "fn" NAME [type_params] "(" [params] ")" ["->" ret_type]
trait_def: "trait" NAME [type_params] ":" _NEWLINE _INDENT trait_item+ _DEDENT
?trait_item: fn_head _NEWLINE | fn_def
impl_def: "impl" type ["for" type] ":" _NEWLINE _INDENT fn_def+ _DEDENT
test_def: "test" STRING ":" suite
{import_rule}

dotted: NAME ("." NAME)*
type_params: "[" type_param ("," type_param)* "]"
type_param: NAME [":" type]
params: param ("," param)* [","]
param: [VAR] NAME [":" type] ["=" test]
ret_type: type ["!" type]

type: base_type QMARK*
?base_type: NAME [type_args]          -> named_type
    | "[" type "]"                    -> list_type
    | "{{" type ":" type "}}"         -> dict_type
    | "(" type ("," type)+ ")"        -> tuple_type
    | "dyn" NAME                      -> dyn_type
    | NONE                            -> unit_type
type_args: "[" type ("," type)* "]"

type_def: "type" NAME [type_params] "(" fields ")" _NEWLINE          -> record_def
    | "type" NAME [type_params] "=" variant ("|" variant)* _NEWLINE   -> sum_def
fields: field ("," field)* [","]
field: NAME ":" type ["=" test]
variant: NAME ["(" fields ")"]

suite: simple_stmt | _NEWLINE _INDENT stmt+ _DEDENT
?stmt: simple_stmt | compound_stmt
simple_stmt: small_stmt _NEWLINE
?small_stmt: var_stmt | expr_stmt | return_stmt | assert_stmt | pass_stmt
    | break_stmt | continue_stmt
var_stmt: VAR NAME [":" type] "=" test
expr_stmt: testlist [annassign | augassign | assign]
annassign: ":" type "=" test
assign: "=" testlist
augassign: AUGOP test
return_stmt: "return" [testlist]
assert_stmt: "assert" test ["," test]
pass_stmt: "pass"
break_stmt: "break"
continue_stmt: "continue"

?compound_stmt: if_stmt | while_stmt | for_stmt | match_stmt
if_stmt: "if" test ":" suite elif_clause* [else_clause]
elif_clause: "elif" test ":" suite
else_clause: "else" ":" suite
while_stmt: "while" test ":" suite
for_stmt: "for" targets "in" testlist ":" suite
targets: NAME ("," NAME)*
match_stmt: "match" test ":" _NEWLINE _INDENT arm+ _DEDENT
arm: {arm_prefix} pattern ":" suite
pattern: NAME "(" [pattern ("," pattern)*] ")" -> variant_pattern
    | NAME                                   -> name_pattern
    | literal                                -> literal_pattern
literal: [ADD_OP] NUMBER | STRING | "True" | "False" | NONE

?testlist: test ("," test)*
?test: coalesce ["if" coalesce "else" test] | lambdef
{lambda_rule}
{coalesce_rule}
?or_test: and_test ("or" and_test)*
?and_test: not_test ("and" not_test)*
?not_test: "not" not_test -> not_op
    | comparison
?comparison: arith (comp_op arith)*
comp_op: "<" | ">" | "==" | ">=" | "<=" | "!=" | "in" | "not" "in" {is_ops}
?arith: term (ADD_OP term)*
?term: factor (MUL_OP factor)*
?factor: ADD_OP factor -> unary
    | power
?power: atom_expr [POW factor]
?atom_expr: atom_expr "(" [arguments] ")" -> call
    | atom_expr "[" subscript "]"         -> getitem
    | atom_expr "." NAME                  -> getattr
    | atom_expr "?"                       -> try_op
    | atom
arguments: argument ("," argument)* [","]
argument: NAME "=" test -> kwarg
    | test [comp_for]
?subscript: test
    | [test] ":" [test] -> slice
?atom: NAME
    | NUMBER
    | STRING+
    | "True"
    | "False"
    | NONE
    | "(" ")"                                 -> unit
    | "(" test ")"
    | "(" test "," [test ("," test)* [","]] ")" -> tuple
    | "(" test comp_for ")"                   -> genexp
    | "[" [test ("," test)* [","]] "]"        -> list
    | "[" test comp_for "]"                   -> listcomp
    | "{{" [dict_item ("," dict_item)* [","]] "}}" -> dict
    | "{{" dict_item comp_for "}}"            -> dictcomp
    | "fail" atom_expr                        -> fail_expr
dict_item: test ":" test
comp_for: "for" targets "in" or_test ("if" or_test)* [comp_for]

VAR: "var"
NONE: "{none}"
AUGOP: "+=" | "-=" | "*=" | "/=" | "//=" | "%="
ADD_OP: "+" | "-"
MUL_OP: "*" | "/" | "//" | "%"
POW.2: "**"
QMARK: "?"
NAME: /[A-Za-z_][A-Za-z0-9_]*/
NUMBER: /\d[\d_]*(\.\d[\d_]*)?([eE][+-]?\d+)?/
STRING: /{string_re}/
COMMENT: /#[^\n]*/
_NEWLINE: ( /\r?\n[\t ]*/ | COMMENT )+

%ignore /[\t \f]+/
%ignore /\\[\t \f]*\r?\n/
%ignore COMMENT
%declare _INDENT _DEDENT
"""

VARIANTS = {
    "a": {
        "import_rule": 'import_stmt: "use" use_part ("." use_part)* _NEWLINE'
        '\nuse_part: NAME | "{" NAME ("," NAME)* "}"',
        "arm_prefix": "",
        "lambda_rule": 'lambdef: NAME "=>" test',
        "coalesce_rule": "?coalesce: or_test",
        "is_ops": "",
        "none": "none",
    },
    "b": {
        "import_rule": 'import_stmt: "from" dotted "import" NAME ("," NAME)* _NEWLINE'
        '\n    | "import" dotted _NEWLINE',
        "arm_prefix": '"case"',
        "lambda_rule": 'lambdef: "lambda" [NAME ("," NAME)*] ":" test',
        "coalesce_rule": '?coalesce: or_test ("??" or_test)*',
        "is_ops": '| "is" | "is" "not"',
        "none": "None",
    },
}

# Python constructs that are not part of either variant.
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

# Python constructs that variant B adopts on purpose and variant A rejects.
A_ONLY_LEAKS = {
    "None": r"\bNone\b",
    "lambda": r"\blambda\b",
    "import": r"^[ \t]*(import|from)\s",
    "case": r"^[ \t]*case\s",
    "is": r"\bis\b",
}

STRINGS_AND_COMMENTS = re.compile(STRING_RE + r"|#[^\n]*")


class XIndenter(Indenter):
    NL_type = "_NEWLINE"
    OPEN_PAREN_types: ClassVar[list[str]] = ["LPAR", "LSQB", "LBRACE"]
    CLOSE_PAREN_types: ClassVar[list[str]] = ["RPAR", "RSQB", "RBRACE"]
    INDENT_type = "_INDENT"
    DEDENT_type = "_DEDENT"
    tab_len = 4


@cache
def parser(variant: str) -> Lark:
    grammar = GRAMMAR.format(string_re=STRING_RE, **VARIANTS[variant])
    return Lark(grammar, parser="lalr", postlex=XIndenter())


def parse_error(variant: str, source: str) -> str | None:
    """The first syntax error in `source` under `variant`, or None when it parses."""
    if not source.endswith("\n"):
        source += "\n"
    try:
        parser(variant).parse(source)
    except LarkError as err:
        return str(err).strip().splitlines()[0]
    return None


def leaks(variant: str, source: str) -> list[str]:
    """Names of Python constructs in `source` that `variant` does not have."""
    code = STRINGS_AND_COMMENTS.sub('""', source)
    patterns = COMMON_LEAKS | (A_ONLY_LEAKS if variant == "a" else {})
    return [
        name
        for name, pattern in patterns.items()
        if re.search(pattern, code, re.MULTILINE)
    ]
