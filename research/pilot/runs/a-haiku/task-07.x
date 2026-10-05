type TransferErr = AccountNotFound(name: str) | InsufficientFunds(name: str, balance: int)

fn withdraw(var ledger: {str: int}, account: str, amount: int) -> none ! TransferErr:
    if account in ledger:
        balance = ledger[account]
        if balance >= amount:
            ledger[account] = balance - amount
        else:
            fail InsufficientFunds(account, balance)
    else:
        fail AccountNotFound(account)

fn deposit(var ledger: {str: int}, account: str, amount: int) -> none ! TransferErr:
    if account in ledger:
        ledger[account] = ledger[account] + amount
    else:
        fail AccountNotFound(account)

fn transfer(var ledger: {str: int}, from_account: str, to_account: str, amount: int) -> none ! TransferErr:
    withdraw(ledger, from_account, amount)?
    deposit(ledger, to_account, amount)?

test "transfer":
    var ledger = {"alice": 100, "bob": 50}
    transfer(ledger, "alice", "bob", 30)?
    assert ledger["alice"] == 70
    assert ledger["bob"] == 80
    assert transfer(ledger, "alice", "bob", 100) == fail InsufficientFunds("alice", 70)
