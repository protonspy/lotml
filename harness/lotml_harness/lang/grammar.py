"""lotml's grammar: the single source the parser is built from and the dialects are generated from.

Written in Lark's notation with `_INDENT` and `_DEDENT` produced by an indenter, as
Python's own grammar is. Variant A and variant B differ in six constructs, filled in from
`VARIANTS`. The grammar avoids what other engines cannot express — terminal priorities,
lookarounds, regular-expression flags — so `dialects` can translate it rule by rule.
"""

from functools import cache
from typing import ClassVar

from lark import Lark
from lark.indenter import Indenter

STRING_RE = (
    r"[rRbBfF]{0,2}("
    r'"""(("|"")?([^"\\]|\\(.|\n)))*"""'
    r"|'''(('|'')?([^'\\]|\\(.|\n)))*'''"
    r'|"([^"\\\n]|\\(.|\n))*"'
    r"|'([^'\\\n]|\\(.|\n))*'"
    r")"
)
"""Strings as Python writes them, in a regular expression every engine reads: no lazy or
possessive repetition, so a triple-quoted string ends at its first unescaped `\"\"\"`."""

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
type_param: NAME [":" NAME]
params: param ("," param)* [","]
param: [INOUT | SINK | VAR] NAME [":" type] ["=" test]
ret_type: type ["!" type]

type: base_type QMARK*
?base_type: NAME [type_args]              -> named_type
    | "[" type "]"                        -> list_type
    | "{{" type ":" type "}}"             -> dict_type
    | "{{" type "}}"                      -> set_type
    | "(" type "," [type ("," type)* [","]] ")" -> tuple_type
    | "dyn" NAME                          -> dyn_type
    | NONE                                -> unit_type
type_args: "[" type ("," type)* "]"

type_def: "type" NAME [type_params] "(" [fields] ")" _NEWLINE          -> record_def
    | "type" NAME [type_params] "=" variant ("|" variant)* _NEWLINE   -> sum_def
fields: field ("," field)* [","]
field: NAME ":" type ["=" test]
variant: NAME ["(" [variant_fields] ")"]
variant_fields: variant_field ("," variant_field)* [","]
?variant_field: field | type

suite: simple_stmt | _NEWLINE _INDENT stmt+ _DEDENT
?stmt: simple_stmt | compound_stmt
simple_stmt: small_stmt _NEWLINE
?small_stmt: var_stmt | expr_stmt | return_stmt | assert_stmt | pass_stmt
    | break_stmt | continue_stmt
var_stmt: VAR NAME [":" type] "=" testlist
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
targets: target ("," target)*
?target: NAME | "(" targets ")"
match_stmt: "match" test ":" _NEWLINE _INDENT arm+ _DEDENT
arm: {arm_prefix} pattern ":" suite
pattern: NAME "(" [pattern ("," pattern)*] ")"   -> variant_pattern
    | NAME                                       -> name_pattern
    | "(" pattern "," [pattern ("," pattern)*] ")" -> tuple_pattern
    | literal                                    -> literal_pattern
literal: [PLUS | MINUS] NUMBER | STRING | TRUE | FALSE | NONE

?testlist: test ("," test)*
?test: coalesce ["if" coalesce "else" test] | lambdef
{lambda_rule}
{coalesce_rule}
?or_test: and_test ("or" and_test)*
?and_test: not_test ("and" not_test)*
?not_test: "not" not_test -> not_op
    | comparison
?comparison: bit_or (comp_op bit_or)*
comp_op: _comparison -> op
    | "in" -> op_in
    | "not" "in" -> op_not_in
    {is_ops}
?bit_or: bit_xor ("|" bit_xor)*
?bit_xor: bit_and ("^" bit_and)*
?bit_and: shift ("&" shift)*
?shift: arith (_shift arith)*
?arith: term (_sign term)*
?term: factor (_product factor)*
?factor: _sign factor -> unary
    | "~" factor -> invert
    | power
?power: atom_expr [POW factor]
?atom_expr: atom_expr "(" [arguments] ")" -> call
    | atom_expr "[" subscript "]"         -> getitem
    | atom_expr "." NAME                  -> getattr
    | atom_expr "?"                       -> try_op
    | atom
arguments: argument ("," argument)* [","]
argument: NAME "=" test -> kwarg
    | "&" atom_expr     -> inout_arg
    | test [comp_for]
?subscript: test
    | [test] ":" [test] [":" [test]] -> slice
?atom: NAME
    | NUMBER
    | STRING+
    | TRUE
    | FALSE
    | NONE
    | "(" ")"                                 -> unit
    | "(" test ")"
    | "(" test "," [test ("," test)* [","]] ")" -> tuple
    | "(" test comp_for ")"                   -> genexp
    | "[" [test ("," test)* [","]] "]"        -> list
    | "[" test comp_for "]"                   -> listcomp
    | "{{" [dict_item ("," dict_item)* [","]] "}}" -> dict
    | "{{" dict_item comp_for "}}"            -> dictcomp
    | "{{" test ("," test)* [","] "}}"        -> set
    | "{{" test comp_for "}}"                 -> setcomp
    | "fail" atom_expr                        -> fail_expr
dict_item: test ":" test
comp_for: "for" targets "in" or_test ("if" or_test)* [comp_for]

INOUT: "inout"
SINK: "sink"
VAR: "var"
NONE: "{none}"
TRUE: "True"
FALSE: "False"
AUGOP: "+=" | "-=" | "*=" | "/=" | "//=" | "%=" | "**=" | "&=" | "|=" | "^=" | "<<=" | ">>="
_comparison: LT | GT | EQ | NE | LE | GE
_shift: LSHIFT | RSHIFT
_sign: PLUS | MINUS
_product: STAR | SLASH | DSLASH | PERCENT
LT: "<"
GT: ">"
EQ: "=="
NE: "!="
LE: "<="
GE: ">="
LSHIFT: "<<"
RSHIFT: ">>"
PLUS: "+"
MINUS: "-"
STAR: "*"
SLASH: "/"
DSLASH: "//"
PERCENT: "%"
POW: "**"
QMARK: "?"
NAME: /[A-Za-z_][A-Za-z0-9_]*/
NUMBER: /0[xX][0-9a-fA-F_]+|0[bB][01_]+|0[oO][0-7_]+|\d[\d_]*(\.\d[\d_]*)?([eE][+-]?\d+)?/
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
        '\nuse_part: NAME | "{{" NAME ("," NAME)* "}}"',
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
        "is_ops": '| "is" -> op_is\n    | "is" "not" -> op_is_not',
        "none": "None",
    },
}


def source(variant: str) -> str:
    """The grammar of `variant` in Lark's notation, indentation left to an indenter."""
    fields = VARIANTS[variant]
    filled = GRAMMAR.replace("{{", "\0").replace("}}", "\1")
    for key, value in fields.items():
        filled = filled.replace("{" + key + "}", value.replace("{{", "\0").replace("}}", "\1"))
    filled = filled.replace("{string_re}", STRING_RE)
    return filled.replace("\0", "{").replace("\1", "}")


class LotmlIndenter(Indenter):
    NL_type = "_NEWLINE"
    OPEN_PAREN_types: ClassVar[list[str]] = ["LPAR", "LSQB", "LBRACE"]
    CLOSE_PAREN_types: ClassVar[list[str]] = ["RPAR", "RSQB", "RBRACE"]
    INDENT_type = "_INDENT"
    DEDENT_type = "_DEDENT"
    tab_len = 4


@cache
def parser(variant: str) -> Lark:
    return Lark(
        source(variant),
        parser="lalr",
        postlex=LotmlIndenter(),
        propagate_positions=True,
        maybe_placeholders=True,
    )
