"""pass@k and the Wilson interval, against values worked by hand (specs/agent-harness/ R4.2)."""

import pytest

from lotml_harness.agent.report import pass_at_k, wilson


def test_pass_at_1_is_the_pass_rate():
    assert pass_at_k(5, 2, 1) == pytest.approx(0.4)
    assert pass_at_k(4, 0, 1) == 0.0
    assert pass_at_k(4, 4, 1) == 1.0


def test_pass_at_k_is_one_minus_the_chance_that_every_draw_fails():
    # 1 - C(3, 2) / C(5, 2) = 1 - 3/10
    assert pass_at_k(5, 2, 2) == pytest.approx(0.7)
    # 1 - C(7, 3) / C(10, 3) = 1 - 35/120
    assert pass_at_k(10, 3, 3) == pytest.approx(85 / 120)


def test_pass_at_k_is_certain_when_fewer_runs_fail_than_are_drawn():
    assert pass_at_k(5, 4, 2) == 1.0
    assert pass_at_k(3, 1, 3) == 1.0


def test_pass_at_k_needs_k_runs():
    with pytest.raises(ValueError):
        pass_at_k(2, 1, 3)
    with pytest.raises(ValueError):
        pass_at_k(3, 4, 1)


def test_wilson_matches_the_worked_values():
    low, high = wilson(8, 10)
    assert (low, high) == (pytest.approx(0.4902, abs=1e-4), pytest.approx(0.9433, abs=1e-4))
    low, high = wilson(0, 10)
    assert low == pytest.approx(0.0, abs=1e-12)
    assert high == pytest.approx(0.2775, abs=1e-4)
    low, high = wilson(10, 10)
    assert (low, high) == (pytest.approx(0.7225, abs=1e-4), pytest.approx(1.0, abs=1e-12))


def test_wilson_of_no_runs_says_nothing():
    assert wilson(0, 0) == (0.0, 1.0)
