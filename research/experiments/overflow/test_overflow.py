import pytest
from overflow import EXPRESSIONS, LOOPS, MAX, MIN, SETUP, trap, wrap


def test_trap_passes_every_i64_and_fails_one_past_either_end():
    assert trap(MAX) == MAX
    assert trap(MIN) == MIN
    with pytest.raises(OverflowError):
        trap(MAX + 1)
    with pytest.raises(OverflowError):
        trap(MIN - 1)


def test_wrap_is_twos_complement():
    assert wrap(MAX + 1) == MIN
    assert wrap(MIN - 1) == MAX
    assert wrap(MAX * 2) == -2
    assert wrap(-5) == -5


def run(statement: str, **values) -> dict:
    scope: dict = {}
    exec(SETUP, scope)  # noqa: S102
    scope.update(values)
    exec(statement, scope)  # noqa: S102
    return scope


@pytest.mark.parametrize("name", sorted(EXPRESSIONS))
def test_every_strategy_computes_the_plain_result_when_nothing_overflows(name):
    for op in "+*":
        assert run(EXPRESSIONS[name].format(op=op))["r"] == run(f"r = a {op} b")["r"]
    assert run(LOOPS[name])["t"] == 500_500


@pytest.mark.parametrize("name", [n for n in EXPRESSIONS if "trap" in n])
def test_every_trap_strategy_fails_on_overflow(name):
    with pytest.raises(OverflowError):
        run(EXPRESSIONS[name].format(op="+"), a=MAX, b=1)


@pytest.mark.parametrize("name", [n for n in EXPRESSIONS if "wrap" in n])
def test_every_wrap_strategy_wraps_on_overflow(name):
    assert run(EXPRESSIONS[name].format(op="+"), a=MAX, b=1)["r"] == MIN
