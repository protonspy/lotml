import tiktoken

from lotml_harness import reference


def test_reference_fits_the_token_budget():
    """R12: the complete specification is under 10,000 tokens."""
    source = reference.text()
    for name in ("o200k_base", "cl100k_base"):
        assert len(tiktoken.get_encoding(name).encode(source)) < reference.TOKEN_BUDGET


def test_reference_is_made_of_examples():
    """R12 adjusted: idiom examples rather than rules — every section shows code."""
    source = reference.text()
    sections = source.split("\n## ")[1:]
    without_code = [s.splitlines()[0] for s in sections if not reference.examples(s)]
    assert without_code == ["Not in the language"]


def test_reference_marks_where_semantics_differ():
    """Every place lotml's meaning differs from Python's is marked."""
    bullets = reference.text().split("\n- ")
    marked = " ".join(b for b in bullets if b.startswith("**Not Python:**")).lower()
    for difference in (
        "only a `var` may be reassigned",
        "overflow",
        "`ys = xs` copies",
        "records are values",
        "no truthiness",
        "no exceptions",
        "must cover every case",
        "`inout`",
        "captures copies",
    ):
        assert difference in marked, difference


def test_reference_covers_the_v1_requirements():
    """R24 conventions, R28 unit type, R33 placeholder, R34 division, R35 prelude."""
    source = reference.text()
    for needle in (
        "inout xs",
        "sink log",
        "&xs",
        "-> None ! E",
        "todo()",
        "`/` always returns f64",
        "The prelude needs no import",
    ):
        assert needle in source, needle


def test_examples_extracts_unlabelled_fences():
    assert reference.examples("x\n```\na = 1\n```\ny\n```\nb\n```\n") == [
        "a = 1\n",
        "b\n",
    ]
