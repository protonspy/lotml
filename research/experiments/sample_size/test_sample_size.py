import random

import pytest
from sample_size import connor_n, exact_power, mcnemar_p, simulated_power


def test_connor_reproduces_the_harness_table():
    assert connor_n(0.20, 0.10) == 155
    assert connor_n(0.10, 0.05) == 312


def test_exact_mcnemar_p_value_is_the_two_sided_binomial_tail():
    assert mcnemar_p(0, 6) == pytest.approx(2 * 0.5**6)
    assert mcnemar_p(1, 9) == pytest.approx(2 * 11 / 1024)
    assert mcnemar_p(5, 5) == 1.0
    assert mcnemar_p(0, 0) == 1.0


def test_exact_power_never_exceeds_alpha_without_a_difference():
    for n in (20, 80, 155):
        assert exact_power(n, 0.10, 0.10) <= 0.05


def test_exact_power_grows_towards_one_with_a_large_difference():
    assert exact_power(40, 0.30, 0.0) > exact_power(20, 0.30, 0.0) > 0.5
    assert exact_power(200, 0.30, 0.0) == pytest.approx(1.0)


def test_simulation_agrees_with_the_exact_power():
    simulated = simulated_power(155, 0.15, 0.05, trials=20_000, rng=random.Random(7))
    assert simulated == pytest.approx(exact_power(155, 0.15, 0.05), abs=0.015)
