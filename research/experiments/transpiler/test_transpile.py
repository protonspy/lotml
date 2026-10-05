import textwrap
from pathlib import Path

import pytest
from transpile import run

PILOT = Path(__file__).parent.parent.parent / "pilot" / "runs"


def outcomes(source: str, variant="b", mode="lotml") -> dict[str, str]:
    result = run(textwrap.dedent(source), variant, mode=mode)
    assert result.error is None, result.error
    return result.tests


def test_a_passing_and_a_failing_test_block():
    assert outcomes("""
        fn add(a: int, b: int) -> int:
            return a + b

        test "adds":
            assert add(2, 3) == 5

        test "wrong":
            assert add(2, 3) == 6
    """) == {"adds": "pass", "wrong": "assertion"}


def test_records_compare_by_value_and_take_defaults():
    assert outcomes("""
        type User(name: str, age: int, email: str? = None)

        test "records":
            u = User("ana", 30)
            assert u == User(name="ana", age=30)
            assert u.email is None
            assert u != User("ana", 31)
    """) == {"records": "pass"}


def test_match_on_sum_type_with_unit_variant_and_literals():
    assert outcomes("""
        type Shape = Circle(r: f64) | Rect(w: f64, h: f64) | Empty

        fn area(s: Shape) -> f64:
            match s:
                case Circle(r):
                    return 3.0 * r * r
                case Rect(w, h):
                    return w * h
                case Empty:
                    return 0.0

        fn sign(n: int) -> str:
            match n:
                case 0:
                    return "zero"
                case -1:
                    return "minus one"
                case _:
                    return "other"

        test "areas":
            assert area(Circle(1.0)) == 3.0
            assert area(Rect(2.0, 3.0)) == 6.0
            assert area(Empty) == 0.0
            assert sign(0) == "zero" and sign(-1) == "minus one" and sign(5) == "other"
    """) == {"areas": "pass"}


def test_a_match_with_no_matching_arm_is_not_silently_skipped():
    assert outcomes("""
        type Light = Red | Green | Blue

        fn name(l: Light) -> str:
            match l:
                case Red:
                    return "red"
                case Green:
                    return "green"

        test "blue":
            assert name(Blue) == "blue"
    """) == {"blue": "non-exhaustive match"}


def test_fail_question_mark_and_coalesce():
    assert outcomes("""
        type ParseErr = Empty | NotNumber(text: str)

        fn parse(s: str) -> int ! ParseErr:
            if s == "":
                fail Empty
            n = s.to_int() ?? fail NotNumber(s)
            return n

        fn double(s: str) -> int ! ParseErr:
            n = parse(s)?
            return n * 2

        fn describe(s: str) -> str:
            match parse(s):
                case Ok(n):
                    return "number"
                case Err(Empty):
                    return "empty"
                case Err(NotNumber(t)):
                    return t

        test "errors":
            assert parse("") == Err(Empty)
            assert parse("x") == Err(NotNumber("x"))
            assert double("21")? == 42
            assert double("x") == Err(NotNumber("x"))
            assert describe("") == "empty" and describe("q") == "q" and describe("4") == "number"

        test "question mark fails a test":
            assert double("nope")? == 0
    """) == {"errors": "pass", "question mark fails a test": "propagated error"}


def test_a_function_with_no_value_returns_ok_none():
    assert outcomes("""
        type E = Bad

        fn check(n: int) -> None ! E:
            if n < 0:
                fail Bad

        test "unit":
            assert check(1) == Ok(None)
            assert check(-1) == Err(Bad)
    """) == {"unit": "pass"}


ALIASING = """
    test "aliasing":
        var a = [1]
        var b = a
        b.append(2)
        assert a == [1]
"""


def test_value_semantics_copies_on_binding_and_python_mode_does_not():
    assert outcomes(ALIASING) == {"aliasing": "pass"}
    assert outcomes(ALIASING, mode="python") == {"aliasing": "assertion"}


VAR_PARAMETER = """
    fn bump(var counts: {str: int}, key: str):
        counts[key] = counts.get(key, 0) + 1

    test "caller sees the change":
        var counts = {"a": 1}
        bump(counts, "a")
        assert counts["a"] == 2
"""


def test_a_var_parameter_is_a_copy_under_value_semantics():
    assert outcomes(VAR_PARAMETER) == {"caller sees the change": "assertion"}
    assert outcomes(VAR_PARAMETER, mode="python") == {"caller sees the change": "pass"}


def test_var_self_method_mutates_its_receiver():
    assert outcomes("""
        type Counter(count: int)

        impl Counter:
            fn get(self) -> int:
                return self.count

            fn bump(var self):
                self.count += 1

            fn zero() -> Counter:
                return Counter(0)

        test "bump":
            var c = Counter.zero()
            c.bump()
            c.bump()
            assert c.get() == 2
    """) == {"bump": "pass"}


def test_spec_builtins_that_python_lacks_or_spells_differently():
    assert outcomes("""
        test "builtins":
            assert "42".to_int() == 42
            assert "x".to_int() is None
            assert "abc".find("z") is None
            assert "abc".find("c") == 2
            assert "k=v".split_once("=") == ("k", "v")
            var xs: [int] = []
            assert xs.pop() is None
            assert [1, 2].last() == 2
            assert [1, 2].contains(2)
            assert {"a": 1}.get("b") is None
    """) == {"builtins": "pass"}


def test_variant_a_or_falls_back_only_on_none_but_python_or_on_zero():
    source = """
        fn pick(x: int?) -> int:
            return x or 5

        test "zero is a value":
            assert pick(0) == 0
            assert pick(none) == 5
    """
    assert outcomes(source, variant="a") == {"zero is a value": "pass"}
    assert outcomes(source, variant="a", mode="python") == {
        "zero is a value": "assertion"
    }


def test_variant_a_compares_with_fail_as_an_error_value():
    assert outcomes(
        """
        type E = Neg(n: int)

        fn check(n: int) -> int ! E:
            if n < 0:
                fail Neg(n)
            return n

        test "fail compares":
            assert check(-2) == fail Neg(-2)
        """,
        variant="a",
    ) == {"fail compares": "pass"}


def test_truthiness_of_a_list_is_a_type_error_in_variant_b():
    source = """
        test "truthy list":
            xs = [1]
            if xs:
                assert True
    """
    assert outcomes(source) == {"truthy list": "type error"}
    assert outcomes(source, mode="python") == {"truthy list": "pass"}


def test_unknown_names_are_reported_as_unresolved():
    assert outcomes("""
        test "lowercase false":
            assert false
    """) == {"lowercase false": "unresolved name"}


def test_failure_traceback_points_at_the_lotml_line():
    result = run(
        'fn f(d: int) -> int:\n    return 10 // d\n\ntest "t":\n    f(0)\n', "b"
    )
    assert result.tests == {"t": "runtime error"}
    shown = result.tracebacks["t"]
    assert "line 2, in f\n    return 10 // d\n" in shown
    assert shown.endswith("ZeroDivisionError: integer division or modulo by zero\n")


def test_an_opus_pilot_program_passes_its_own_tests():
    source = (PILOT / "b-opus" / "task-07.x").read_text(encoding="utf-8")
    tests = run(source, "b").tests
    assert tests and set(tests.values()) == {"pass"}


@pytest.mark.parametrize("variant", ["a", "b"])
def test_every_corpus_program_passes_its_own_tests(variant):
    corpus = Path(__file__).parent.parent.parent / "tokens" / "corpus"
    ran = 0
    for path in sorted(corpus.glob(f"*/{variant}.x")):
        result = run(path.read_text(encoding="utf-8"), variant, path=str(path))
        assert result.error is None, (path, result.error)
        assert set(result.tests.values()) <= {"pass"}, (path, result.tests)
        ran += len(result.tests)
    assert ran >= 12


def test_a_test_that_exceeds_its_step_budget_times_out():
    source = 'test "forever":\n    var i = 0\n    while i < 3:\n        i = i\n'
    assert run(source, "b", budget=10_000).tests == {"forever": "timeout"}


def test_imports_outside_the_research_subset_are_refused():
    result = run('from os import getcwd\n\ntest "t":\n    assert True\n', "b")
    assert result.error is not None and "import" in result.error


def test_dunder_names_are_refused_even_inside_f_strings():
    assert run('test "t":\n    x = ().__class__\n', "b").error is not None
    source = 'test "t":\n    s = f"{__import__(\'os\').getcwd()}"\n'
    assert run(source, "b").error is not None


def test_dangerous_builtins_are_not_in_scope():
    assert run('test "t":\n    open("x")\n', "b").tests == {"t": "unresolved name"}
