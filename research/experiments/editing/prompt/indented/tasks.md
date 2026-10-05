# Editing tasks

- `01-find-adult.lotml` — Add a variant `NoEmail(name: str)` to `LookupErr`. `find_adult` must fail with `NoEmail(name)` when the adult user it found has no email.
- `02-shape-area.lotml` — Add a shape `Square(side: f64)` to `Shape` and make `area` handle it.
- `03-top-words.lotml` — `top_words` must ignore words shorter than 3 letters.
- `04-binary-search.lotml` — When `target` occurs more than once, `binary_search` must return the index of its first occurrence.
- `05-parse-config.lotml` — `parse_config` must fail with `ParseError(number, "duplicate key")` when a key appears a second time.
- `06-bank-transfer.lotml` — Add a variant `SameAccount(account: str)` to `BankErr`. `transfer` must fail with `SameAccount(source)`, changing nothing, when `source` and `target` are the same account.
- `07-stack.lotml` — Add a method `pop_n(var self, n: int) -> [T]` to `Stack` that pops up to `n` items and returns them, most recent first; it stops early when the stack is empty.
- `08-merge-intervals.lotml` — `merge` must skip any interval whose `start` is greater than its `end`.
- `09-json-tree.lotml` — Add a function `count_numbers(j: Json) -> int` that returns how many `Num` values the tree contains, at any depth.
- `10-fetch-retry.lotml` — Add a field `calls: int = 0` to `Flaky` and make its `fetch` increment it on every call, whether the call fails or succeeds.
- `11-priced-report.lotml` — A `Book` whose title starts with "Free" has price 0.0.
- `12-shortest-path.lotml` — `shortest_path` must ignore edges with a negative weight.
