"""The guide's confidence threshold, calibrated on the validation split for a target precision
(plans/harness-guide.md 2.4)."""

import pytest

from lotml_harness.guide.calibrate import threshold


def test_the_threshold_is_the_lowest_confidence_whose_answers_above_it_reach_the_precision():
    pairs = [(0.95, True), (0.9, True), (0.8, True), (0.7, False), (0.6, True), (0.5, False)]
    assert threshold(pairs, 0.75) == 0.6, "answering at 0.6 or above: 4 right of 5"
    assert threshold(pairs, 0.9) == 0.8, "at 0.8 or above: 3 of 3"
    assert threshold(pairs, 1.0) == 0.8


def test_ties_at_one_confidence_are_taken_or_left_together():
    pairs = [(0.9, True), (0.7, True), (0.7, False), (0.7, False)]
    assert threshold(pairs, 0.75) == 0.9, "0.7 brings in two wrong answers with one right"


def test_a_target_no_threshold_reaches_keeps_the_guide_silent():
    assert threshold([(0.9, False), (0.8, False)], 0.5) == 1.0
    assert threshold([], 0.5) == 1.0


def test_a_target_outside_zero_to_one_is_refused():
    with pytest.raises(ValueError):
        threshold([(0.9, True)], 1.5)
