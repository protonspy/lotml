type LedgerErr = AccountNotFound | InsufficientBalance

fn withdraw(var ledger: {str: int}, account: str, amount: int) -> None ! LedgerErr:
    if account not in ledger:
        fail AccountNotFound
    if ledger[account] < amount:
        fail InsufficientBalance
    ledger[account] -= amount

fn deposit(var ledger: {str: int}, account: str, amount: int) -> None ! LedgerErr:
    if account not in ledger:
        fail AccountNotFound
    ledger[account] += amount

fn transfer(var ledger: {str: int}, from_account: str, to_account: str, amount: int) -> None ! LedgerErr:
    withdraw(ledger, from_account, amount)?
    deposit(ledger, to_account, amount)?

test "failed transfer":
    var ledger = {"alice": 100, "bob": 50}
    result = transfer(ledger, "alice", "bob", 150)
    match result:
        case Err(InsufficientBalance):
            pass
        case _:
            assert false
    assert ledger["alice"] == 100
