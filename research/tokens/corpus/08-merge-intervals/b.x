type Interval(start: int, end: int)

fn merge(intervals: [Interval]) -> [Interval]:
    var result: [Interval] = []
    for iv in sorted(intervals, key=lambda iv: iv.start):
        if len(result) > 0 and iv.start <= result[-1].end:
            last = result[-1]
            result[-1] = Interval(last.start, max(last.end, iv.end))
        else:
            result.append(iv)
    return result

test "merge":
    ivs = [Interval(1, 3), Interval(8, 10), Interval(2, 6)]
    assert merge(ivs) == [Interval(1, 6), Interval(8, 10)]
