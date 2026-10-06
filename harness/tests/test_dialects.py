"""The three published dialects of lotml's grammar, each tested against the whole corpus."""

from functools import cache

import pytest
import tiktoken
from lark.exceptions import LarkError
from llguidance import LLMatcher
from llguidance.gbnf_to_lark import any_to_lark
from llguidance.tiktoken import lltokenizer_from_encoding

from lotml_harness import ROOT, reference
from lotml_harness.lang import dialects
from lotml_harness.lang.grammar import parser

CORPUS = ROOT / "research" / "tokens" / "corpus"
PILOT = ROOT / "research" / "pilot" / "runs"


def parses(text: str) -> bool:
    try:
        parser("b").parse(text)
    except LarkError:
        return False
    return True


def corpus() -> dict[str, str]:
    """Every variant B program the parser accepts: corpus, reference examples, pilot runs."""
    programs = {f"corpus/{p.parent.name}": p.read_text("utf-8") for p in CORPUS.glob("*/b.x")}
    for index, example in enumerate(reference.examples(reference.text())):
        programs[f"reference/{index}"] = reference.program(example)
    for path in PILOT.glob("b-*/*.x"):
        programs[f"pilot/{path.parent.name}/{path.stem}"] = path.read_text("utf-8")
    programs = {k: v if v.endswith("\n") else v + "\n" for k, v in programs.items()}
    return {k: v for k, v in sorted(programs.items()) if parses(v)}


PROGRAMS = corpus()

REJECTED = {
    "python def": "def f():\n    pass\n",
    "missing colon": "fn f()\n    pass\n",
    "body not indented": "fn f():\nreturn 1\n",
    "indented one level too deep": "fn f():\n    x = 1\n        y = 2\n",
    "dedent to no level": "fn f():\n    if x:\n        y = 1\n      z = 2\n",
    "unclosed bracket": "fn f():\n    x = (1\n",
    "braces block": "fn f() {\n    return 1\n}\n",
    "keyword glued to a name": "fn f():\n    ifTrue:\n        pass\n",
    "try": "fn f():\n    try:\n        pass\n",
}

COMPACT = {
    "comparisons without spaces": "fn f():\n    y = x<y and a==b or c>=d\n",
    "arithmetic without spaces": "fn f():\n    y = xs[i-1]*2+f(x)//3\n",
    "assignment without spaces": "fn f():\n    var y=1\n    y+=2\n",
    "word operators": "fn f():\n    y = x in xs and x not in ys and x is not None\n",
    "spaces inside brackets": "fn f():\n    y = g( x , [ 1 ] , { 1: 2 } )\n",
}
"""Spacing models write that is not canonical but must never be forbidden."""

NOT_CANONICAL = {
    "indented by two spaces": "fn f():\n  return 1\n",
    "indented with a tab": "fn f():\n\treturn 1\n",
}
"""Indentation the tolerant parser accepts and constrained decoding must not produce."""


def nested(depth: int) -> str:
    lines = ["fn f():"]
    for level in range(1, depth):
        lines.append("    " * level + "if True:")
    lines.append("    " * depth + "pass")
    return "\n".join(lines) + "\n"


def test_the_corpus_is_large_enough_to_mean_something():
    assert len(PROGRAMS) >= 50
    assert {k.split("/")[0] for k in PROGRAMS} == {"corpus", "reference", "pilot"}


# EBNF -------------------------------------------------------------------------------


@cache
def from_ebnf():
    return dialects.lark_from_ebnf(dialects.ebnf("b"))


def ebnf_accepts(text: str) -> bool:
    try:
        from_ebnf().parse(text)
    except LarkError:
        return False
    return True


@pytest.mark.parametrize("name", PROGRAMS)
def test_the_ebnf_read_back_accepts_every_program(name):
    assert ebnf_accepts(PROGRAMS[name])


@pytest.mark.parametrize("name", REJECTED)
def test_the_ebnf_read_back_rejects_what_the_parser_rejects(name):
    text = REJECTED[name]
    assert not parses(text)
    assert not ebnf_accepts(text)


def test_the_ebnf_names_its_lexer_tokens_and_has_no_lark_notation():
    text = dialects.ebnf("b")
    assert "INDENT" in text and "DEDENT" in text and "NEWLINE ::=" in text
    assert "start ::=" in text
    body = text.split("*/", 1)[1]
    assert "%ignore" not in body and "%declare" not in body
    assert "->" not in body.replace('"->"', "")


@pytest.mark.parametrize("name", NOT_CANONICAL)
def test_the_ebnf_accepts_the_indentation_the_parser_tolerates(name):
    assert parses(NOT_CANONICAL[name])
    assert ebnf_accepts(NOT_CANONICAL[name])


# llguidance and GBNF ------------------------------------------------------------------


@cache
def tokenizer():
    encoding = tiktoken.get_encoding("o200k_base")
    return lltokenizer_from_encoding(encoding, eos_token=encoding.eot_token)


@cache
def compiled(kind: str) -> str:
    text = dialects.llguidance("b") if kind == "lark" else any_to_lark(dialects.gbnf("b"))
    return LLMatcher.grammar_from_lark(text)


def accepts(kind: str, text: str) -> bool:
    matcher = LLMatcher(tokenizer(), compiled(kind), log_level=0)
    if matcher.is_error():
        raise AssertionError(matcher.get_error())
    matcher.consume_tokens(tokenizer().tokenize_str(text))
    return not matcher.is_error() and matcher.is_accepting()


@pytest.mark.parametrize("kind", ["lark", "gbnf"])
def test_the_constrained_dialects_compile_in_llguidance(kind):
    assert LLMatcher.validate_grammar(compiled(kind), tokenizer()) == ""


@pytest.mark.parametrize("kind", ["lark", "gbnf"])
@pytest.mark.parametrize("name", PROGRAMS)
def test_the_constrained_dialects_accept_every_program(kind, name):
    assert accepts(kind, PROGRAMS[name])


@pytest.mark.parametrize("kind", ["lark", "gbnf"])
@pytest.mark.parametrize("name", COMPACT)
def test_the_constrained_dialects_accept_compact_spacing(kind, name):
    assert parses(COMPACT[name])
    assert accepts(kind, COMPACT[name])


@pytest.mark.parametrize("kind", ["lark", "gbnf"])
@pytest.mark.parametrize("name", [*REJECTED, *NOT_CANONICAL])
def test_the_constrained_dialects_reject_broken_and_non_canonical_programs(kind, name):
    assert not accepts(kind, (REJECTED | NOT_CANONICAL)[name])


@pytest.mark.parametrize("kind", ["lark", "gbnf"])
def test_blocks_are_bounded_in_depth(kind):
    assert accepts(kind, nested(dialects.DEPTH))
    assert not accepts(kind, nested(dialects.DEPTH + 1))
    assert parses(nested(dialects.DEPTH + 1))


def test_the_llguidance_dialect_uses_no_feature_llguidance_refuses():
    text = dialects.llguidance("b")
    assert "%declare" not in text and "%ignore" not in text
    assert ".2:" not in text and "*?" not in text and "*+" not in text


def test_gbnf_rule_names_are_lowercase_and_dashed():
    import re

    names = re.findall(r"^([^\s#]\S*) ::=", dialects.gbnf("b"), re.MULTILINE)
    assert "root" in names
    assert all(re.fullmatch(r"[a-z][a-z0-9-]*", n) for n in names)


# Regular expressions to grammar notation ----------------------------------------------


@pytest.mark.parametrize(
    ("pattern", "gbnf"),
    [
        ("abc", '"abc"'),
        ("[A-Za-z_][A-Za-z0-9_]*", "[A-Za-z_] [A-Za-z0-9_]*"),
        (r"#[^\n]*", '"#" [^\\n]*'),
        ("a|bc", '("a" | "bc")'),
        ("(ab)+c?", '("ab")+ "c"?'),
        ("x{0,2}", '"x"{0,2}'),
        (r"\d", "[0-9]"),
        (r'"', '"\\""'),
        (r"\\", '"\\\\"'),
        (".", "[^\\n]"),
    ],
)
def test_regex_becomes_gbnf(pattern, gbnf):
    assert dialects.regex_to_gbnf(pattern) == gbnf


@pytest.mark.parametrize(
    ("pattern", "ebnf"),
    [
        ("abc", '"abc"'),
        ("[A-Za-z_][A-Za-z0-9_]*", "[A-Za-z_] [A-Za-z0-9_]*"),
        (r"#[^\n]*", '"#" [^#xA]*'),
        ("a|bc", '( "a" | "bc" )'),
        ("x{0,2}", '( "x" "x"? )?'),
        ('"', "'\"'"),
    ],
)
def test_regex_becomes_ebnf(pattern, ebnf):
    assert dialects.regex_to_ebnf(pattern) == ebnf


@pytest.mark.parametrize("pattern", ["(?=a)b", "a*?", "(a)\\1", "a*+"])
def test_regex_features_other_engines_lack_are_refused(pattern):
    with pytest.raises(dialects.Unportable):
        dialects.regex_to_gbnf(pattern)


# Publishing ---------------------------------------------------------------------------


def test_the_published_grammars_are_generated_from_the_current_source():
    for path, text in dialects.published().items():
        assert path.read_text(encoding="utf-8") == text, f"{path}: run dialects.write()"


# tree-sitter ------------------------------------------------------------------------------


def test_tree_sitter_writes_each_construct_as_grammar_js_does():
    grammar = dialects.load("b")
    name = dialects.Sym("NAME", terminal=True)
    assert dialects.ts_expression(dialects.Rep(name, 0, 1), grammar) == "optional($.name)"
    assert dialects.ts_expression(dialects.Rep(name, 1, None), grammar) == "repeat1($.name)"
    assert dialects.ts_expression(dialects.Rep(name, 2, 3), grammar) == (
        "seq($.name, $.name, optional($.name))"
    )
    assert dialects.ts_expression(dialects.Sym("INOUT", terminal=True), grammar) == "'inout'"
    assert dialects.ts_expression(dialects.Lit("it's"), grammar) == r"'it\'s'"
    assert dialects.ts_expression(dialects.Re("a/b"), grammar) == r"/a\/b/"


def test_tree_sitter_hides_what_lark_inlines_and_keeps_names_apart():
    grammar = dialects.load("b")
    assert dialects.ts_name("start", grammar) == "source_file"
    assert dialects.ts_name("_NEWLINE", grammar) == "_newline"
    assert dialects.ts_name("NAME", grammar) == "name"
    assert dialects.ts_name("fn_def", grammar) == "fn_def"
    assert dialects.ts_name("item", grammar) == "_item", "`?item` leaves no node in Lark"
    assert dialects.ts_name("comparison", grammar) == "_comparison_rule", (
        "the source has `_comparison`"
    )
    text = dialects.tree_sitter("b")
    assert text.count("    _comparison: ") == 1 and text.count("    _comparison_rule: ") == 1


def test_the_highlights_name_the_keywords_of_the_grammar():
    assert {"fn", "match", "case", "parallel"} - set(dialects.keywords("b")) == {"parallel"}
    text = dialects.highlights("b")
    assert '"fn"' in text and '"None" "True" "False"] @constant.builtin' in text


def tree_sitter_cli():
    """`npx` to run tree-sitter's CLI, which compiles the parser with the platform's compiler;
    None where there is none, or where LOTML_SKIP_TREE_SITTER is set."""
    import os
    import shutil

    if os.environ.get("LOTML_SKIP_TREE_SITTER"):
        return None
    return shutil.which("npx")


@pytest.mark.skipif(tree_sitter_cli() is None, reason="needs npx for tree-sitter's CLI")
def test_tree_sitter_parses_the_corpus_and_the_reference_without_an_error(tmp_path):
    import shutil
    import subprocess

    grammar = tmp_path / "tree-sitter"
    shutil.copytree(
        dialects.TREE_SITTER,
        grammar,
        ignore=shutil.ignore_patterns("parser.c", "*.json", "tree_sitter"),
    )
    shutil.copy(dialects.TREE_SITTER / "tree-sitter.json", grammar / "tree-sitter.json")
    cli = [tree_sitter_cli(), "--yes", "tree-sitter-cli@0.27.0"]
    generated = subprocess.run(  # noqa: S603 - the CLI, run on the generated grammar
        [*cli, "generate"], cwd=grammar, capture_output=True, text=True, check=False
    )
    assert generated.returncode == 0, generated.stderr
    sources = tmp_path / "sources"
    sources.mkdir()
    for path in CORPUS.glob("*/b.x"):
        (sources / f"{path.parent.name}.lotml").write_text(
            path.read_text("utf-8"), encoding="utf-8"
        )
    for i, example in enumerate(reference.examples(reference.text())):
        (sources / f"reference-{i}.lotml").write_text(reference.program(example), encoding="utf-8")
    files = sorted(str(p) for p in sources.glob("*.lotml"))
    parsed = subprocess.run(  # noqa: S603 - the CLI, run on the corpus
        [*cli, "parse", "--quiet", *files], cwd=grammar, capture_output=True, text=True, check=False
    )
    errors = [line for line in parsed.stdout.splitlines() if "ERROR" in line or "MISSING" in line]
    assert parsed.returncode == 0 and not errors, "\n".join(errors[:10]) or parsed.stderr
