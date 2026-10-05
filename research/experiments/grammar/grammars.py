"""Grammars for constrained decoding of lotml, checked with llguidance.

llguidance is the engine behind OpenAI's grammar-constrained custom tools, llama.cpp,
vLLM and SGLang. Its Lark dialect has no `%declare` and no post-lexer, so the pilot's
`INDENT`/`DEDENT` grammar cannot be given to it as is. This module tests the two ways
around that on the block structure alone — each logical line is opaque:

- braces: blocks are `{`/`}`, a context-free grammar with unbounded nesting;
- bounded indentation: one copy of the block rules per nesting level up to a bound, each
  level's lines starting with exactly that many spaces.
"""

import re
import statistics
import sys
import time
from functools import cache
from pathlib import Path

import tiktoken
from llguidance import LLMatcher
from llguidance.tiktoken import lltokenizer_from_encoding

sys.path.insert(0, str(Path(__file__).parent.parent / "indentation"))
from slips import MASK, bracket_rows, to_braces
from transpile import GRAMMAR, STRING_RE, VARIANTS

STEP = 4


@cache
def tokenizer():
    encoding = tiktoken.get_encoding("o200k_base")
    return lltokenizer_from_encoding(encoding, eos_token=encoding.eot_token)


def pilot_grammar() -> str:
    """The pilot's own variant B grammar, written for Python Lark with its Indenter."""
    return GRAMMAR.format(string_re=STRING_RE, **VARIANTS["b"])


def validate(grammar: str) -> str:
    """llguidance's verdict on a Lark grammar: empty when it compiles."""
    return LLMatcher.validate_grammar(LLMatcher.grammar_from_lark(grammar), tokenizer())


# The pilot grammar's Python-Lark features, each with a stand-in llguidance accepts.
PILOT_FEATURES = [
    ("terminal priority", "POW.2:", "POW:"),
    (
        "%declare",
        "%declare _INDENT _DEDENT",
        '_INDENT: "<indent>"\n_DEDENT: "<dedent>"',
    ),
]


def pilot_incompatibilities() -> list[tuple[str, str]]:
    """Each feature llguidance refuses in the pilot grammar, with its first error line."""
    grammar, found = pilot_grammar(), []
    for feature, used, stand_in in PILOT_FEATURES:
        error = validate(grammar)
        if not error:
            break
        found.append((feature, error.strip().splitlines()[0]))
        grammar = grammar.replace(used, stand_in)
    if validate(grammar):
        found.append(("other", validate(grammar).strip().splitlines()[0]))
    return found


# A token-level grammar that tries exact indentation with newline terminals, while
# `%ignore` lets spaces in anywhere, as a full grammar of the language would.
IGNORED_SPACES = r"""start: stmt*
stmt: NL0 WORD | NL0 WORD ":" block1
block1: (NL1 WORD)+
NL0: /\n/
NL1: /\n    /
WORD: /[a-z]+/
%ignore /[ ]+/
"""


def matcher(grammar: str) -> LLMatcher:
    return LLMatcher(tokenizer(), LLMatcher.grammar_from_lark(grammar), log_level=0)


def accepts(grammar: str, text: str) -> bool:
    match = matcher(grammar)
    if match.is_error():
        return False
    match.consume_tokens(tokenizer().tokenize_str(text))
    return not match.is_error() and match.is_accepting()


def skeleton(source: str, braces: bool = False) -> str:
    """`source` without comments, one physical line per statement, optionally in braces."""
    code = MASK.sub(lambda m: "" if m.group().startswith("#") else m.group(), source)
    lines = code.split("\n")
    out: list[str] = []
    for index, starts in bracket_rows(code):
        line = lines[index].rstrip()
        if not line.strip():
            continue
        if starts or not out:
            out.append(line)
        else:
            out[-1] += " " + line.strip()
    text = "\n".join(out) + "\n"
    return to_braces(text) if braces else text


def depth(program: str) -> int:
    """How many block levels deep `program` goes, the outermost level being zero."""
    widths = [
        len(line) - len(line.lstrip()) for line in program.split("\n") if line.strip()
    ]
    return max(widths, default=0) // STEP


BRACES = r"""start: (item | BLANK)*
item: LINE | HEADER body
body: (item | BLANK)* (CLOSE | CONTINUE body)
LINE: /[ \t]*[^ \t\n}]([^\n]*[^ \t\n{])?\n/
HEADER: /[ \t]*[^ \t\n}][^\n]*\{\n/
CONTINUE: /[ \t]*\}[ \t]*(else|elif)[^\n]*\{\n/
CLOSE: /[ \t]*\}\n/
BLANK: /[ \t]*\n/
"""


def braces_grammar() -> str:
    return BRACES


def indentation_grammar(bound: int) -> str:
    """Blocks nested at most `bound` levels deep, by exact indentation per level."""
    rules = ["start: (item0 | BLANK)*"]
    for level in range(bound + 1):
        rules.append(
            f"item{level}: LINE{level} | HEADER{level} block{level + 1}"
            if level < bound
            else f"item{level}: LINE{level}"
        )
        if level > 0:
            rules.append(f"block{level}: BLANK* item{level} (item{level} | BLANK)*")
        spaces = " " * (STEP * level)
        rules.append(f"LINE{level}: /{spaces}[^ \\t\\n]([^\\n]*[^ \\t\\n:])?\\n/")
        if level < bound:
            rules.append(f"HEADER{level}: /{spaces}[^ \\t\\n][^\\n]*:\\n/")
    rules.append(r"BLANK: /[ \t]*\n/")
    return "\n".join(rules)


def mask_times(grammar: str, text: str) -> list[float]:
    """Microseconds to compute each token mask while the text is forced through."""
    match = matcher(grammar)
    times = []
    for token in tokenizer().tokenize_str(text):
        start = time.perf_counter()
        match.compute_bitmask()
        times.append((time.perf_counter() - start) * 1e6)
        if not match.consume_token(token):
            raise ValueError(match.get_error())
    return times


def main() -> None:
    import importlib.metadata

    programs = [
        p.read_text(encoding="utf-8")
        for p in sorted(Path(__file__).parent.parent.parent.glob("tokens/corpus/*/b.x"))
    ]
    indented = [skeleton(p) for p in programs]
    braced = [skeleton(p, braces=True) for p in programs]
    deepest = max(depth(p) for p in indented)
    version = importlib.metadata.version("llguidance")
    lines = [
        "# Grammars for constrained decoding",
        "",
        (
            f"Generated by `grammars.py` with llguidance {version} and the `o200k`"
            f" tokenizer, over the {len(programs)} variant B programs of the paired corpus"
            f" reduced to their block structure (deepest: {deepest} levels)."
        ),
        "",
        "## The pilot's grammar",
        "",
        "llguidance refuses these Python-Lark features, in the order it reports them:",
        "",
    ]
    lines += [f"- {feature}: `{error}`" for feature, error in pilot_incompatibilities()]
    lines += [
        "",
        "With both replaced the grammar compiles, but `INDENT` and `DEDENT` become literal",
        "tokens no real program contains: the indentation has to be in the grammar itself.",
        "",
        "## Ignored spaces defeat exact indentation",
        "",
        (
            "A token-level grammar with `%ignore` for spaces and newline terminals carrying"
            " exact indentation accepts a line indented one level too deep:"
            f" `{accepts(IGNORED_SPACES, chr(10) + 'if:' + chr(10) + ' ' * 8 + 'a')}`."
            " The bounded grammars below are line-oriented instead, so each level's lines"
            " are matched whole."
        ),
        "",
        "## Size and acceptance",
        "",
        "| grammar | rules | bytes | corpus accepted |",
        "| --- | ---: | ---: | ---: |",
    ]
    candidates = [("braces", braces_grammar(), braced)]
    candidates += [
        (f"indentation, bound {b}", indentation_grammar(b), indented)
        for b in (deepest, 8, 16, 32)
    ]
    for name, grammar, texts in candidates:
        rules = len(re.findall(r"^\S", grammar, re.MULTILINE))
        accepted = sum(accepts(grammar, t) for t in texts)
        lines.append(
            f"| {name} | {rules} | {len(grammar)} | {accepted} of {len(texts)} |"
        )
    lines += [
        "",
        "## Mask computation per token",
        "",
        "| grammar | tokens | median µs | 99th percentile µs |",
        "| --- | ---: | ---: | ---: |",
    ]
    for name, grammar, texts in candidates:
        times = [t for text in texts for t in mask_times(grammar, text)]
        p99 = statistics.quantiles(times, n=100)[98]
        lines.append(
            f"| {name} | {len(times)} | {statistics.median(times):.0f} | {p99:.0f} |"
        )
    (Path(__file__).parent / "results.md").write_text("\n".join(lines) + "\n", "utf-8")


if __name__ == "__main__":
    main()
