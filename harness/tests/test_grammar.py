import pytest
from lark.exceptions import LarkError

from lotml_harness import ROOT, reference
from lotml_harness.lang.grammar import parser, source
from lotml_harness.lang.transpile import transpile

CORPUS = ROOT / "research" / "tokens" / "corpus"


def parses(text: str, variant: str = "b") -> bool:
    try:
        parser(variant).parse(text if text.endswith("\n") else text + "\n")
    except LarkError:
        return False
    return True


@pytest.mark.parametrize("example", reference.examples(reference.text()))
def test_every_reference_example_parses_and_transpiles(example):
    program = reference.program(example)
    assert parses(program), example
    for mode in ("lotml", "python"):
        compile(transpile(program, "b", mode), "example.lotml", "exec")


def test_program_puts_loose_statements_in_a_function():
    example = "type P(x: int)\n\nimpl P:\n    fn f(self):\n\n        pass\nx = 1\n"
    assert reference.program(example) == (
        "type P(x: int)\n\nimpl P:\n    fn f(self):\n\n        pass\n\nfn example():\n    x = 1\n"
    )


@pytest.mark.parametrize("program", sorted(CORPUS.glob("*/b.x")), ids=lambda p: p.parent.name)
def test_the_paired_corpus_parses_in_variant_b(program):
    assert parses(program.read_text(encoding="utf-8"), "b")


@pytest.mark.parametrize("program", sorted(CORPUS.glob("*/a.x")), ids=lambda p: p.parent.name)
def test_the_paired_corpus_parses_in_variant_a(program):
    assert parses(program.read_text(encoding="utf-8"), "a")


@pytest.mark.parametrize(
    "snippet",
    [
        "fn f(inout xs: [int], sink ys: [str], var n: int) -> {str: (int, f64?)}:\n    pass",
        "fn f() -> int ! E:\n    return x ** 2 * 3 // 4 % 5 << 1 >> 2 | 3 ^ 4 & ~5",
        "fn f():\n    s = {1, 2}\n    d = {}\n    c = {k for k in xs}\n    t = (1,)",
        "fn f():\n    z = xs[::-1]\n    y = xs[1:]\n    add(&xs, &self.items, &ys[0])",
        "fn f():\n    for i, (a, b) in enumerate(ps):\n        pass",
        "fn f():\n    match t:\n        case (a, -1):\n            pass\n        case Num(n):\n"
        "            pass",
        "type Expr = Num(int) | Add(Expr, Expr) | Neg(e: Expr)",
        "type Stack[T](items: [T])\nimpl Stack[T]:\n    fn push(inout self, item: T):\n"
        "        self.items.append(item)",
        "fn f():\n    n = 0x1F + 0b101 + 0o7 + 1_000 + 1e-9",
        'fn f() -> str:\n    """Docs."""\n    return f"{x:.2f}"',
    ],
)
def test_the_reference_constructs_parse(snippet):
    assert parses(snippet)


@pytest.mark.parametrize(
    "snippet",
    [
        "def f():\n    pass",
        "fn f():\n    try:\n        pass",
        "fn f()\n    pass",
        "fn f():\nreturn 1",
        "fn f():\n    x = (1",
        "fn f():\n    if x { y }",
    ],
)
def test_python_and_c_family_forms_do_not_parse(snippet):
    assert not parses(snippet)


def test_the_variants_differ_where_variant_b_takes_python_syntax():
    a_only = (
        "use math.{sqrt}\nfn f():\n    g = x => x\n    match v:\n        none:\n            pass"
    )
    b_only = (
        "from math import sqrt\nfn f():\n    g = lambda x: x\n"
        "    match v:\n        case None:\n            pass"
    )
    assert parses(a_only, "a") and not parses(a_only, "b")
    assert parses(b_only, "b") and not parses(b_only, "a")
    assert parses("fn f():\n    y = x ?? 0", "b")
    assert not parses("fn f():\n    y = x ?? 0", "a")


@pytest.mark.parametrize("variant", ["a", "b"])
def test_a_closing_bracket_without_its_opening_one_is_a_parse_error(variant):
    with pytest.raises(LarkError):
        parser(variant).parse("fn f():\n    x = 1)\n")


def test_threads_parse_independently():
    """The indenter keeps state while it parses, so threads must not share one parser."""
    from concurrent.futures import ThreadPoolExecutor

    programs = [p.read_text(encoding="utf-8") for p in sorted(CORPUS.glob("*/b.x"))] * 8

    def outcome(text):
        return parses(text)

    with ThreadPoolExecutor(max_workers=8) as pool:
        assert all(pool.map(outcome, programs))


def test_source_fills_each_variant_without_leftover_placeholders():
    for variant in "ab":
        text = source(variant)
        assert "{import_rule}" not in text and "{none}" not in text
        assert '"{" type ":" type "}"' in text
