Add taking stock out to `inventory.lot`:

- `type StockErr = Unknown(name: str) | Short(name: str, missing: int)`
- `fn take(inout items: [Item], name: str, qty: int) -> None ! StockErr` removes `qty` of `name`. It fails with `Unknown(name)` when there is no such item, and with `Short(name, missing)` when there are fewer than `qty`, `missing` being how many are lacking; a failed take leaves the items as they were.
- `fn take_all(inout items: [Item], orders: [(str, int)]) -> None ! StockErr` takes every `(name, qty)` order, or, failing with the first order's error, none of them: on an error the items are as they were before the call.

Add tests for both.
