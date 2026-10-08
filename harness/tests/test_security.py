"""What model-written programs must not reach: the runtime, the environment, the parent's result."""

import base64
import json
import pickle
import sys
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
        name for name in namespace if name not in ("A", "B", "T") and not name.startswith("__")
    }
    assert internal == set()


@pytest.mark.parametrize(
    "source",
    [
        'fn f(x: int) -> str:\n    return "{0.__class__}".format(x)\n\ntest "t":\n    s = f(1)\n',
        'fn f(x: int) -> str:\n    return "{a.__class__}".format_map({"a": x})\n\n'
        'test "t":\n    s = f(1)\n',
    ],
)
def test_format_cannot_walk_into_dunder_attributes(source):
    result = run(source)
    assert result.error is not None or set(result.tests.values()) != {"pass"}


def test_ordinary_format_still_works():
    source = 'test "t":\n    assert "{} and {x}".format(1, x=2) == "1 and 2"\n'
    assert run(source).tests == {"t": "pass"}


@pytest.mark.parametrize(
    "body",
    [
        'f = "{0.__class__}".format\n    s = f(1)',
        's = str.format("{0.__class__}", 1)',
        'f = "{a.__class__}".format_map\n    s = f({"a": 1})',
    ],
)
def test_format_taken_as_a_value_cannot_walk_into_dunder_attributes(body):
    result = run(f'test "t":\n    {body}\n')
    assert result.tests == {"t": "type error"}


@pytest.mark.parametrize("mode", ["lotml", "python"])
@pytest.mark.parametrize("field", ["0.gi_frame", "0.gi_code", "0.gi_frame.f_back"])
def test_format_fields_cannot_walk_into_the_interpreter(mode, field):
    source = f'test "t":\n    s = "{{{field}}}".format(y for y in [1])\n'
    assert run(source, mode=mode).tests == {"t": "type error"}


def test_a_field_named_format_can_still_be_assigned():
    source = (
        'type R(format: str)\n\ntest "t":\n'
        '    var r = R("a")\n    r.format = "b"\n    assert r.format == "b"\n'
    )
    assert run(source).tests == {"t": "pass"}


def test_format_taken_as_a_value_still_formats():
    source = 'test "t":\n    f = "{} {}".format\n    assert f(1, 2) == "1 2"\n'
    assert run(source).tests == {"t": "pass"}


@pytest.mark.parametrize(
    "attribute", ["gi_frame", "gi_code", "f_back", "f_globals", "f_locals", "cr_frame", "tb_frame"]
)
def test_attributes_into_the_interpreter_are_refused(attribute):
    source = f'test "t":\n    g = (y for y in [1])\n    x = g.{attribute}\n'
    assert run(source).error.startswith("transpile:")
    in_string = f'test "t":\n    g = (y for y in [1])\n    x = f"{{g.{attribute}}}"\n'
    assert run(in_string).error.startswith("transpile:")


def test_the_builtin_pow_traps_before_computing():
    result = isolated('test "t":\n    x = pow(10, 10 ** 9)\n', timeout=10)
    assert result.tests == {"t": "overflow"}


@pytest.mark.parametrize(
    "source",
    [
        'fn f(x: int) -> int:\n    return x\n\ntest "t":\n    y = f(__x=1)\n',
        "fn f(x: int) -> int:\n    match x:\n        case __A(y):\n            return y\n",
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


def test_the_compiler_a_child_runs_never_downloads_a_python(monkeypatch):
    monkeypatch.delenv("LOTML_PYTHON", raising=False)
    monkeypatch.setenv("LOTML_OFFLINE", "")
    environment = execute.child_environment()
    assert environment["LOTML_OFFLINE"] == "1", "an empty LOTML_OFFLINE does not turn it off"
    assert environment["LOTML_PYTHON"] == sys.executable


def test_an_exit_from_a_test_is_a_failure_not_a_clean_exit():
    budget = execute.Budget(compile("pass", "p", "exec"))

    def leave():
        raise SystemExit(0)

    outcome, _, error = execute.attempt(budget, 100, leave)
    assert outcome == "runtime error" and isinstance(error, SystemExit)


def test_memory_is_limited():
    source = 'test "t":\n    s = "a" * 3000000000\n    assert len(s) > 0\n'
    result = isolated(source, memory=2**30)
    assert result.tests == {"t": "runtime error"}


def test_private_tests_refuse_a_decompression_bomb():
    payload = base64.b64encode(zlib.compress(pickle.dumps("[" + "0," * 10**7 + "0]"))).decode()
    with pytest.raises(pickle.UnpicklingError):
        decode_private(payload, limit=1_000_000)
