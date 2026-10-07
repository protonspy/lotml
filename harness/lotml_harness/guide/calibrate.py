"""The guide's confidence threshold: below it the guide says nothing (plans/harness-guide.md 2.4).

Calibrated on the validation split, never the held-out one: the lowest confidence at which the
answers the guide would give — those at or above it — are right at least as often as the target
precision. A lower threshold answers more often; this is the lowest that keeps the promise.
"""


def threshold(pairs: list[tuple[float, bool]], target: float) -> float:
    """The threshold for `target` precision from (confidence, first location right) pairs; 1.0,
    which keeps the guide silent, when no threshold reaches it."""
    if not 0.0 <= target <= 1.0:
        raise ValueError(f"a precision lies between 0 and 1, not {target}")
    chosen = 1.0
    right = answered = 0
    for confidence in sorted({c for c, _ in pairs}, reverse=True):
        at = [ok for c, ok in pairs if c == confidence]
        right += sum(at)
        answered += len(at)
        if right / answered >= target:
            chosen = confidence
    return chosen
