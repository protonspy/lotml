def binary_search(xs: list[int], target: int) -> int | None:
    lo, hi = 0, len(xs) - 1
    while lo <= hi:
        mid = (lo + hi) // 2
        if xs[mid] == target:
            return mid
        if xs[mid] < target:
            lo = mid + 1
        else:
            hi = mid - 1
    return None


def test_binary_search() -> None:
    xs = [1, 3, 5, 7, 9]
    assert binary_search(xs, 7) == 3
    assert binary_search(xs, 4) is None
