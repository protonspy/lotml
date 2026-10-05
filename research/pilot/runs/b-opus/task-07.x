# Task 7: a ledger of account balances with withdraw, deposit and transfer.

type LedgerErr = NoAccount(name: str) | Insufficient(name: str, balance: int, amount: int) | BadAmount(amount: int)

type Ledger(balances: {str: int})

impl Ledger:
    fn balance(self, name: str) -> int ! LedgerErr:
        b = self.balances.get(name) ?? fail NoAccount(name)
        return b

    fn deposit(var self, name: str, amount: int) -> None ! LedgerErr:
        if amount <= 0:
            fail BadAmount(amount)
        current = self.balances.get(name) ?? fail NoAccount(name)
        self.balances[name] = current + amount

    fn withdraw(var self, name: str, amount: int) -> None ! LedgerErr:
        if amount <= 0:
            fail BadAmount(amount)
        current = self.balances.get(name) ?? fail NoAccount(name)
        if current < amount:
            fail Insufficient(name, current, amount)
        self.balances[name] = current - amount

    fn transfer(var self, src: str, dst: str, amount: int) -> None ! LedgerErr:
        # Check the destination first so a failed deposit never loses withdrawn money.
        if dst not in self.balances:
            fail NoAccount(dst)
        self.withdraw(src, amount)?
        self.deposit(dst, amount)?

test "transfer with insufficient balance fails and changes nothing":
    var l = Ledger({"ana": 100, "bob": 50})
    assert l.transfer("ana", "bob", 500) == Err(Insufficient("ana", 100, 500))
    assert l.balance("ana")? == 100
    assert l.balance("bob")? == 50

test "transfer to a missing account fails and changes nothing":
    var l = Ledger({"ana": 100})
    assert l.transfer("ana", "zed", 10) == Err(NoAccount("zed"))
    assert l.balance("ana")? == 100

test "transfer from a missing account fails":
    var l = Ledger({"bob": 50})
    assert l.transfer("zed", "bob", 10) == Err(NoAccount("zed"))
    assert l.balance("bob")? == 50

test "successful transfer moves the amount":
    var l = Ledger({"ana": 100, "bob": 50})
    l.transfer("ana", "bob", 30)?
    assert l.balance("ana")? == 70
    assert l.balance("bob")? == 80

test "withdraw and deposit":
    var l = Ledger({"ana": 100})
    l.deposit("ana", 20)?
    l.withdraw("ana", 70)?
    assert l.balance("ana")? == 50
    assert l.withdraw("ana", 51) == Err(Insufficient("ana", 50, 51))
    assert l.withdraw("bob", 1) == Err(NoAccount("bob"))
