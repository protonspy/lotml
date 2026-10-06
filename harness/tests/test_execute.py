import json
import textwrap

import pytest

from lotml_harness import ROOT
from lotml_harness.execute import Result, isolated, run
from lotml_harness.tasks import Case, Task
from lotml_harness.tasks.types import List, Prim, Tuple

PILOT = ROOT / "research" / "pilot" / "runs"
CORPUS = ROOT / "research" / "tokens" / "corpus"
RESEARCH = ROOT / "research" / "experiments" / "transpiler" / "results.json"


def outcomes(source: str, variant="b", mode="lotml", **options) -> dict[str, str]:
    result = run(textwrap.dedent(source), variant, mode, **options)
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


def test_match_on_sum_types_literals_and_tuples():
    assert outcomes("""
        type Shape = Circle(r: f64) | Rect(w: f64, h: f64) | Empty
        type Expr = Num(int) | Add(Expr, Expr)

        fn area(s: Shape) -> f64:
            match s:
                case Circle(r):
                    return 3.0 * r * r
                case Rect(w, h):
                    return w * h
                case Empty:
                    return 0.0

        fn eval(e: Expr) -> int:
            match e:
                case Num(n):
                    return n
                case Add(a, b):
                    return eval(a) + eval(b)

        fn sign(pair: (int, int)) -> str:
            match pair:
                case (0, _):
                    return "zero"
                case (-1, y):
                    return f"minus {y}"
                case _:
                    return "other"

        test "matches":
            assert area(Circle(1.0)) == 3.0 and area(Rect(2.0, 3.0)) == 6.0
            assert area(Empty) == 0.0
            assert eval(Add(Num(2), Add(Num(3), Num(4)))) == 9
            assert sign((0, 1)) == "zero" and sign((-1, 2)) == "minus 2"
            assert sign((5, 5)) == "other"
    """) == {"matches": "pass"}


def test_a_match_with_no_matching_arm_stops_the_program():
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

        fn check(s: str) -> None ! ParseErr:
            parse(s)?

        test "errors":
            assert parse("") == Err(Empty)
            assert parse("x") == Err(NotNumber("x"))
            assert double("21")? == 42
            assert check("1") == Ok(None) and check("") == Err(Empty)

        test "question mark fails a test":
            assert double("nope")? == 0
    """) == {"errors": "pass", "question mark fails a test": "propagated error"}


ALIASING = """
    type P(x: int)

    fn wrap(xs: [int]) -> [[int]]:
        return [xs]

    test "aliasing":
        var a = [1]
        b = a
        nested = wrap(a)
        var p = P(1)
        q = p
        a.append(2)
        p.x = 5
        assert b == [1] and nested == [[1]] and q.x == 1
"""


def test_value_semantics_copies_on_binding_and_python_mode_does_not():
    assert outcomes(ALIASING) == {"aliasing": "pass"}
    assert outcomes(ALIASING, mode="python") == {"aliasing": "assertion"}


INOUT = """
    type Bag(items: [int])

    fn add_all(inout xs: [int], values: [int]):
        for v in values:
            xs.append(v)

    fn reset(inout n: int):
        n = 0

    fn bump(var counts: {str: int}, key: str):
        counts[key] = counts.get(key, 0) + 1

    test "inout writes back":
        var xs = [1]
        add_all(&xs, [2, 3])
        var n = 5
        reset(&n)
        var bag = Bag([])
        add_all(&bag.items, [7])
        var grid = [[0], [1]]
        add_all(&grid[1], [2])
        assert xs == [1, 2, 3] and n == 0 and bag.items == [7] and grid == [[0], [1, 2]]

    test "var parameter is a copy":
        var counts = {"a": 1}
        bump(counts, "a")
        assert counts["a"] == 1
"""


def test_inout_arguments_reach_the_caller_and_var_parameters_do_not():
    assert outcomes(INOUT) == {"inout writes back": "pass", "var parameter is a copy": "pass"}
    assert outcomes(INOUT, mode="python") == {
        "inout writes back": "assertion",
        "var parameter is a copy": "assertion",
    }


def test_methods_static_functions_and_trait_defaults():
    assert outcomes("""
        type Counter(count: int)

        impl Counter:
            fn zero() -> Counter:
                return Counter(0)

            fn bump(inout self):
                self.count += 1

        trait Show:
            fn show(self) -> str

            fn loud(self) -> str:
                return self.show().upper()

        impl Show for Counter:
            fn show(self) -> str:
                return f"counter {self.count}"

        type Stack[T](items: [T])

        impl Stack[T]:
            fn push(inout self, item: T):
                self.items.append(item)

        test "methods":
            var c = Counter.zero()
            c.bump()
            c.bump()
            assert c.show() == "counter 2" and c.loud() == "COUNTER 2"
            var s = Stack([1])
            s.push(2)
            assert s.items == [1, 2]
    """) == {"methods": "pass"}


def test_numbers_follow_python_except_that_int_traps():
    assert outcomes("""
        test "division":
            assert 7 / 2 == 3.5 and -7 // 2 == -4 and -7 % 2 == 1
            assert (6 & 3) | 8 == 10 and 1 << 4 == 16 and ~0 == -1 and 5 ^ 1 == 4
            assert 2 ** 10 == 1024 and 0x10 == 16 and 1_000 == 1000

        test "overflow":
            var big = 9223372036854775807
            big += 1

        test "overflow in an expression":
            x = 2 ** 64

        test "negation overflows":
            x = -(-9223372036854775807 - 1)
    """) == {
        "division": "pass",
        "overflow": "overflow",
        "overflow in an expression": "overflow",
        "negation overflows": "overflow",
    }


def test_collections_slices_and_comprehensions():
    assert outcomes("""
        test "collections":
            xs = [3, 1, 2]
            assert xs[::-1] == [2, 1, 3] and xs[1:] == [1, 2]
            assert [x * x for x in xs if x > 1] == [9, 4]
            assert {x % 2 for x in xs} == {0, 1} and len({1, 2, 2}) == 2
            assert {k: len(k) for k in ["ab"]} == {"ab": 2}
            assert sum(x for x in xs) == 6
            assert reversed(xs) == [2, 1, 3] and sorted(xs, key=lambda x: -x) == [3, 2, 1]
            var total = 0
            for i, (a, b) in enumerate([(1, 2), (3, 4)]):
                total += i * (a + b)
            assert total == 7
    """) == {"collections": "pass"}


def test_lotml_methods_that_differ_from_python():
    assert outcomes("""
        test "builtins":
            assert "42".to_int() == 42 and "x".to_int() is None
            assert "abc".find("z") is None and "abc".find("c") == 2
            assert "k=v".split_once("=") == ("k", "v")
            var xs: [int] = []
            assert xs.pop() is None
            assert [1, 2].last() == 2 and [1, 2].contains(2) and [1, 2].index(5) is None
            assert {"a": 1}.get("b") is None
            h = Heap([3, 1])
            assert h.peek() == 1
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
    assert outcomes(source, variant="a", mode="python") == {"zero is a value": "assertion"}


def test_variant_a_compares_with_fail_and_imports_with_use():
    assert outcomes(
        """
        use math.{sqrt}
        type E = Neg(n: int)

        fn check(n: int) -> int ! E:
            if n < 0:
                fail Neg(n)
            return n

        test "fail compares":
            assert check(-2) == fail Neg(-2)
            assert sqrt(4.0) == 2.0
            f = x => x + 1
            assert f(1) == 2
        """,
        variant="a",
    ) == {"fail compares": "pass"}


def test_truthiness_is_a_type_error_in_variant_b():
    source = """
        test "truthy list":
            xs = [1]
            if xs:
                assert True
    """
    assert outcomes(source) == {"truthy list": "type error"}
    assert outcomes(source, mode="python") == {"truthy list": "pass"}


def test_placeholders_unknown_names_and_runaway_loops():
    assert outcomes(
        """
        test "todo":
            x = todo()

        test "lowercase false":
            assert false

        test "forever":
            while True:
                pass
        """,
        budget=10_000,
    ) == {"todo": "todo", "lowercase false": "unresolved name", "forever": "timeout"}


def test_a_failure_traceback_points_at_the_lotml_line():
    result = run('fn f(d: int) -> int:\n    return 10 // d\n\ntest "t":\n    f(0)\n')
    assert result.tests == {"t": "runtime error"}
    shown = result.tracebacks["t"]
    assert 'program.lotml", line 2, in f\n    return 10 // d\n' in shown
    assert shown.endswith("ZeroDivisionError: integer division or modulo by zero\n")


@pytest.mark.parametrize(
    ("source", "error"),
    [
        ("fn f(:\n", "parse"),
        ('from os import getcwd\n\ntest "t":\n    assert True\n', "transpile"),
        ('test "t":\n    x = ().__class__\n', "transpile"),
        ('test "t":\n    s = f"{__import__(\'os\')}"\n', "transpile"),
        ("type T = A\ntype U(x: int = missing)\n", None),
    ],
)
def test_programs_that_never_run_say_why(source, error):
    result = run(source)
    if error is None:
        assert result.error is None
    else:
        assert result.error.startswith(error + ":")
        assert not result.passed


def test_dangerous_builtins_are_not_in_scope():
    assert run('test "t":\n    open("x")\n').tests == {"t": "unresolved name"}


def test_mutability_violations_fail_a_program_that_runs():
    result = run('test "t":\n    xs = [1]\n    xs.append(2)\n    assert len(xs) == 2\n')
    assert result.tests == {"t": "pass"}
    assert result.violations == ["mutate immutable `xs`"]
    assert not result.passed
    assert run('test "t":\n    xs = [1]\n    xs.append(2)\n', mode="python").passed


def task(returns=None, compare="eq") -> Task:
    return Task(
        id="t/1",
        source="t",
        name="pair_sum",
        params=[("xs", List(Prim("int")))],
        returns=returns or Tuple((Prim("int"), Prim("int"))),
        doc="",
        tests=[Case([[1, 2]], (3, 2), compare), Case([[]], (0, 0), compare)],
    )


def test_a_task_runs_its_hidden_cases_against_the_function():
    source = "fn pair_sum(xs: [int]) -> (int, int):\n    return (sum(xs), len(xs))\n"
    result = run(source, task=task())
    assert result.cases == ["pass", "pass"] and result.passed
    wrong = "fn pair_sum(xs: [int]) -> (int, int):\n    return (sum(xs), 1 // len(xs))\n"
    result = run(wrong, task=task())
    assert result.cases == ["wrong answer", "runtime error"] and not result.passed
    assert "1 // len(xs)" in result.tracebacks["1"]


def test_hidden_cases_get_fresh_arguments_each_time():
    source = "fn pair_sum(inout xs: [int]) -> (int, int):\n    xs.append(1)\n    return (3, 2)\n"
    assert run(source, task=task(), mode="python").cases[0] == "pass"


def test_a_task_whose_function_is_missing_never_runs():
    result = run("fn other() -> int:\n    return 1\n", task=task())
    assert result.error == "load: no function `pair_sum`"


def test_isolated_runs_in_a_child_and_reports_the_same_result():
    source = "fn pair_sum(xs: [int]) -> (int, int):\n    print(xs)\n    return (sum(xs), len(xs))\n"
    assert isolated(source, task=task()) == run(source, task=task())


def test_isolated_survives_hangs_and_crashes():
    hang = 'test "t":\n    var x = [0]\n    while True:\n        x.append(1)\n'
    assert isolated(hang, budget=10**12, timeout=3).error.startswith("timeout")
    assert Result(**json.loads(json.dumps(Result(error="x").__dict__))) == Result(error="x")


def test_an_opus_pilot_program_passes_its_own_tests():
    source = (PILOT / "b-opus" / "task-07.x").read_text(encoding="utf-8")
    tests = run(source, "b").tests
    assert tests and set(tests.values()) == {"pass"}


@pytest.mark.parametrize("variant", ["a", "b"])
def test_every_corpus_program_passes_its_own_tests(variant):
    ran = 0
    for path in sorted(CORPUS.glob(f"*/{variant}.x")):
        result = run(path.read_text(encoding="utf-8"), variant, path=str(path))
        assert result.error is None, (path, result.error)
        assert set(result.tests.values()) <= {"pass"}, (path, result.tests)
        ran += len(result.tests)
    assert ran >= 12


def test_the_pilot_runs_keep_the_research_transpilers_outcomes():
    """The executor grew from research/experiments/transpiler and must not regress on it."""
    changed = []
    for row in json.loads(RESEARCH.read_text(encoding="utf-8")):
        if row["error"] is not None:
            continue
        program = PILOT / row["run"] / f"{row['task']}.x"
        source = program.read_text(encoding="utf-8")
        for mode in ("lotml", "python"):
            tests = run(source, row["run"][0], mode, budget=200_000).tests
            expected = {name: t[mode] for name, t in row["tests"].items()}
            if tests != expected:
                changed.append((row["run"], row["task"], mode, tests, expected))
    assert changed == []
