from semantics import findings


def kinds(variant: str, src: str) -> list[str]:
    return [kind for kind, _ in findings(variant, src)]


def test_reassigning_an_immutable_local_is_flagged():
    src = (
        "fn f() -> int:\n    total = 0\n    for x in [1]:\n"
        "        total = total + x\n    return total\n"
    )
    assert kinds("a", src) == ["reassign immutable"]


def test_reassigning_a_var_local_is_fine():
    src = (
        "fn f() -> int:\n    var total = 0\n    for x in [1]:\n"
        "        total += x\n    return total\n"
    )
    assert kinds("a", src) == []


def test_mutating_call_on_an_immutable_local_is_flagged():
    src = "fn f() -> [int]:\n    xs = []\n    xs.append(1)\n    return xs\n"
    assert kinds("b", src) == ["mutate immutable"]


def test_mutating_self_without_var_self_is_flagged():
    src = "type C(n: int)\n\nimpl C:\n    fn bump(self):\n        self.n += 1\n"
    assert kinds("a", src) == ["mutate immutable"]


def test_mutating_var_self_is_fine():
    src = "type C(n: int)\n\nimpl C:\n    fn bump(var self):\n        self.n += 1\n"
    assert kinds("a", src) == []


def test_declaring_in_sibling_branches_is_fine():
    src = (
        "fn f(c: bool) -> int:\n    if c:\n        x = 1\n    else:\n"
        "        x = 2\n    return 0\n"
    )
    assert kinds("a", src) == []


def test_truthiness_of_a_list_is_flagged_in_both_variants():
    src = "fn f(xs: [int]) -> bool:\n    if not xs:\n        return True\n    return False\n"
    assert kinds("a", src) == ["truthiness"]
    assert kinds("b", src) == ["truthiness"]


def test_truthiness_of_an_optional_is_flagged_only_in_variant_b():
    src = "fn f(x: int?) -> bool:\n    if x:\n        return True\n    return False\n"
    assert kinds("a", src) == []
    assert kinds("b", src) == ["truthiness"]


def test_mutable_parameter_is_reported():
    src = 'fn put(var d: {str: int}):\n    d["a"] = 1\n'
    assert kinds("a", src) == ["var parameter"]


def test_handwritten_corpus_breaks_no_rule():
    from pathlib import Path

    corpus = Path(__file__).parent.parent / "tokens" / "corpus"
    for path in sorted(corpus.glob("*/[ab].x")):
        found = kinds(path.stem, path.read_text(encoding="utf-8"))
        assert set(found) <= {"var parameter"}, path
