type FetchErr(url: str)

trait Fetcher:
    fn fetch(var self, url: str) -> str ! FetchErr

fn fetch_with_retry[F: Fetcher](var fetcher: F, url: str, attempts: int) -> str ! FetchErr:
    var last = FetchErr(url)
    for _ in range(attempts):
        match fetcher.fetch(url):
            Ok(body):
                return body
            Err(err):
                last = err
    fail last

type Flaky(failures: int)

impl Fetcher for Flaky:
    fn fetch(var self, url: str) -> str ! FetchErr:
        if self.failures > 0:
            self.failures -= 1
            fail FetchErr(url)
        return "ok"

test "retry":
    assert fetch_with_retry(Flaky(2), "x", 3)? == "ok"
    assert fetch_with_retry(Flaky(5), "x", 3) == fail FetchErr("x")
