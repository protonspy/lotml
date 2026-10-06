import textwrap

import pytest

from lotml_harness.lang.check import leaks, violations


def found(source: str, variant: str = "b") -> list[str]:
    return violations(variant, textwrap.dedent(source))


def test_reassigning_an_immutable_local_is_found():
    assert found("""
        fn total(xs: [int]) -> int:
            total = 0
            for x in xs:
                total = total + x
            return total
    """) == ["reassign immutable `total`"]


def test_a_var_may_be_reassigned_and_mutated():
    assert (
        found("""
        fn total(xs: [int]) -> int:
            var total = 0
            var seen = []
            for x in xs:
                total += x
                seen.append(x)
                seen[0] = x
            return total
    """)
        == []
    )


@pytest.mark.parametrize(
    ("body", "finding"),
    [
        ("xs.append(1)", "mutate immutable `xs`"),
        ("xs[0] = 1", "mutate immutable `xs`"),
        ("p.x = 1.0", "mutate immutable `p`"),
        ("p.move()", "mutate immutable `p`"),
        ("bump(&n)", "`&n` of an immutable"),
        ("n += 1", "reassign immutable `n`"),
    ],
)
def test_mutating_a_plain_parameter_is_found(body, finding):
    source = f"""
        type P(x: f64)
        impl P:
            fn move(inout self):
                self.x += 1.0
        fn f(xs: [int], p: P, n: int):
            {body}
    """
    assert found(source) == [finding]


def test_inout_and_var_parameters_are_mutable():
    assert (
        found("""
        fn f(inout xs: [int], var n: int):
            xs.append(n)
            n += 1
            xs = []
    """)
        == []
    )


def test_loop_and_match_bindings_are_immutable():
    assert found("""
        fn f(xs: [int], e: E):
            for x in xs:
                x = 1
            match e:
                case Num(n):
                    n = 2
    """) == ["reassign immutable `x`", "reassign immutable `n`"]


def test_test_blocks_are_checked():
    assert found("""
        test "t":
            xs = [1]
            xs.append(2)
    """) == ["mutate immutable `xs`"]


def test_tuple_targets_and_unknown_names_are_handled():
    assert found("""
        fn f():
            a, b = 1, 2
            a = 3
            unknown.x = 1
    """) == ["reassign immutable `a`"]


def test_python_constructs_are_leaks_in_both_variants():
    source = "def f(x: Optional[int]):\n    raise ValueError()\n# def in a comment\n"
    assert leaks("b", source) == ["def", "raise", "Optional["]


def test_variant_b_syntax_is_a_leak_in_variant_a():
    source = "from math import sqrt\nf = lambda x: x is None\n"
    assert leaks("a", source) == ["None", "lambda", "import", "is"]
    assert leaks("b", source) == []


def test_strings_do_not_leak():
    assert leaks("b", 's = "class def raise"\n') == []
