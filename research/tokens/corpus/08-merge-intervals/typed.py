from dataclasses import dataclass


@dataclass(frozen=True)
class Interval:
    start: int
    end: int


def merge(intervals: list[Interval]) -> list[Interval]:
    result: list[Interval] = []
    for iv in sorted(intervals, key=lambda iv: iv.start):
        if result and iv.start <= result[-1].end:
            last = result[-1]
            result[-1] = Interval(last.start, max(last.end, iv.end))
        else:
            result.append(iv)
    return result


def test_merge() -> None:
    ivs = [Interval(1, 3), Interval(8, 10), Interval(2, 6)]
    assert merge(ivs) == [Interval(1, 6), Interval(8, 10)]
