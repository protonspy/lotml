In `ledger.lot`, `deposit_all` and `replay` each repeat the rules of a transaction. Extract them into `fn apply(inout acct: Account, tx: Tx) -> None ! TxErr`, which applies one transaction and leaves the account unchanged when it fails, and make both functions use it without changing what they do.

Then add `fn total_withdrawn(txs: [Tx]) -> int`, the sum of the withdrawals' amounts.
