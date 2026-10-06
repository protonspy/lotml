"""What model-written programs must not reach: the runtime, the environment, the parent's result."""

import base64
import json
import pickle
import time
import zlib

import pytest

from lotml_harness import execute
from lotml_harness.execute import isolated, run
from lotml_harness.tasks.livecodebench import decode_private


def fails(source: str, **options) -> bool:
    result = run(source, **options)
    return not result.passed


@pytest.mark.parametrize(
    "body",
    [
        "x = _rt.builtins.open",
        "x = _rt.dataclasses",
        "x = _variants",
        "_tests.append(1)",
        "x = rt",
    ],
)
def test_the_runtime_and_the_programs_bookkeeping_are_out_of_reach(body):
    assert fails(f'test "escape":\n    {body}\n    assert True\n')


def test_no_name_the_transpiler_generates_can_be_written_by_a_program():
    source = 'type T = A | B\ntest "t":\n    x = [A, B]\n    assert len(x) == 2\n'
    loaded = execute.load(source, "b", "lotml", "p.lotml")
    namespace, _code = loaded
    internal = {
        name
        for name in namespace
        if name not in ("A", "B", "T") and not name.startswith("__")
    }
    assert internal == set()


@pytest.mark.parametrize(
    "source",
    [
        'fn f(x: int) -> str:\n    return "{0.__class__}".format(x)\n\ntest "t":\n    s = f(1)\n',
        'fn f(x: int) -> str:\n    return "{a.__class__}".format_map({"a": x})\n\ntest "t":\n    s = f(1)\n',
    ],
)
def test_format_cannot_walk_into_dunder_attributes(source):
    result = run(source)
    assert result.error is not None or set(result.tests.values()) != {"pass"}


def test_ordinary_format_still_works():
    source = 'test "t":\n    assert "{} and {x}".format(1, x=2) == "1 and 2"\n'
    assert run(source).tests == {"t": "pass"}


@pytest.mark.parametrize(
    "source",
    [
        'fn f(x: int) -> int:\n    return x\n\ntest "t":\n    y = f(__x=1)\n',
        'fn f(x: int) -> int:\n    match x:\n        case __A(y):\n            return y\n',
    ],
)
def test_keyword_and_pattern_names_are_guarded(source):
    assert run(source).error is not None


@pytest.mark.parametrize("body", ["x = 10 ** (10 ** 9)", "x = 1 << (10 ** 9)", "x = 3 ** 100"])
def test_huge_powers_and_shifts_overflow_without_being_computed(body):
    start = time.perf_counter()
    assert run(f'test "t":\n    {body}\n').tests == {"t": "overflow"}
    assert time.perf_counter() - start < 5


def test_output_is_capped():
    source = 'test "t":\n    for i in range(100000):\n        print("x" * 1000)\n'
    assert run(source).tests == {"t": "panic"}


def test_a_forged_result_line_is_ignored():
    fake = json.dumps({"error": None, "tests": {"t": "pass"}, "cases": [], "tracebacks": {}})
    source = f'test "t":\n    print({json.dumps(fake)})\n    assert False\n'
    assert isolated(source).tests == {"t": "assertion"}


def test_the_child_sees_only_the_environment_it_needs(monkeypatch):
    monkeypatch.setenv("LOTML_SECRET_FOR_TEST", "x")
    environment = execute.child_environment()
    assert "LOTML_SECRET_FOR_TEST" not in environment
    assert "PYTHONPATH" in environment


def test_an_exit_from_a_test_is_a_failure_not_a_clean_exit():
    budget = execute.Budget(compile("pass", "p", "exec"))

    def leave():
        raise SystemExit(0)

    outcome, _, error = execute.attempt(budget, 100, leave)
    assert outcome == "runtime error" and isinstance(error, SystemExit)


def test_memory_is_limited():
    source = 'test "t":\n    s = "a" * 3000000000\n    assert len(s) > 0\n'
    result = isolated(source, memory=512 * 2**20)
    assert result.tests == {"t": "runtime error"}


def test_private_tests_refuse_a_decompression_bomb():
    payload = base64.b64encode(zlib.compress(pickle.dumps("[" + "0," * 10**7 + "0]"))).decode()
    with pytest.raises(pickle.UnpicklingError):
        decode_private(payload, limit=1_000_000)
