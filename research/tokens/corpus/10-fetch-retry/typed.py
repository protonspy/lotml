from typing import Protocol

import pytest


class FetchError(Exception):
    pass


class Fetcher(Protocol):
    def fetch(self, url: str) -> str: ...


def fetch_with_retry(fetcher: Fetcher, url: str, attempts: int) -> str:
    last = FetchError(url)
    for _ in range(attempts):
        try:
            return fetcher.fetch(url)
        except FetchError as err:
            last = err
    raise last


class Flaky:
    def __init__(self, failures: int) -> None:
        self.failures = failures

    def fetch(self, url: str) -> str:
        if self.failures > 0:
            self.failures -= 1
            raise FetchError(url)
        return "ok"


def test_retry() -> None:
    assert fetch_with_retry(Flaky(2), "x", 3) == "ok"
    with pytest.raises(FetchError):
        fetch_with_retry(Flaky(5), "x", 3)
